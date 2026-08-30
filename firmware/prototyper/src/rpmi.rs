//! RPMI shared-memory mailbox transport.
//!
use alloc::{boxed::Box, vec, vec::Vec};
use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicU16, Ordering},
};

use log::warn;
use rpmi::message::{MessageHeader, MessageType, Status};
use spin::Mutex;

/// Size of an RPMI message header.
pub const RPMI_MSG_HDR_SIZE: usize = 8;
/// Default transfer timeouts in milliseconds.
pub const RPMI_DEF_TX_TIMEOUT: u32 = 500;
pub const RPMI_DEF_RX_TIMEOUT: u32 = 500;

const QUEUE_HEADER_SLOTS: usize = 2;
const MAILBOX_DOORBELL_TRIGGER_OFFSET: usize = 0x40;
const MAILBOX_INT_EN_REG_OFFSET: usize = 0x118;

/// Discovers and constructs the platform RPMI shared-memory mailbox.
pub fn discover_mailbox(root: &serde_device_tree::buildin::Node) -> Option<&'static RpmiMailbox> {
    let timebase_frequency = root
        .find("/cpus")?
        .get_prop("timebase-frequency")?
        .deserialize::<u32>();
    if timebase_frequency == 0 {
        warn!("rpmi-shmem-mbox: invalid timebase frequency");
        return None;
    }
    let mut mailbox = None;
    crate::devicetree::search_with_parent(root, &mut |node, _| {
        if mailbox.is_some() {
            return;
        }
        let Some(compatible) = node.get_prop("compatible") else {
            return;
        };
        let compatible = compatible.deserialize::<serde_device_tree::buildin::StrSeq>();
        if !compatible
            .iter()
            .any(|value| value == "riscv,rpmi-shmem-mbox")
        {
            return;
        }
        let Some(slot_size) = crate::platform::prop_u32_cells(node, "riscv,slot-size")
            .and_then(|cells| cells.first().copied())
            .map(|value| value as usize)
        else {
            warn!("rpmi-shmem-mbox: missing slot size");
            return;
        };
        let Some(reg) = node.get_prop("reg") else {
            warn!("rpmi-shmem-mbox: missing register regions");
            return;
        };
        let reg = reg.deserialize::<serde_device_tree::buildin::Reg>();
        let ranges: Vec<_> = reg.iter().map(|entry| entry.0).collect();
        if slot_size < 64
            || !slot_size.is_power_of_two()
            || slot_size - RPMI_MSG_HDR_SIZE > u16::MAX as usize
            || ranges.len() < 5
        {
            warn!("rpmi-shmem-mbox: invalid queue layout");
            return;
        }
        let Some(headers_size) = QUEUE_HEADER_SLOTS.checked_mul(slot_size) else {
            warn!("rpmi-shmem-mbox: invalid slot size");
            return;
        };
        let Some(queue_min_size) = slot_size
            .checked_mul(2)
            .and_then(|slots_size| headers_size.checked_add(slots_size))
        else {
            warn!("rpmi-shmem-mbox: invalid slot size");
            return;
        };
        if ranges[..4].iter().any(|range| {
            range.start % slot_size != 0
                || range.len() < queue_min_size
                || (range.len() - headers_size) % slot_size != 0
        }) || ranges[0].len() != ranges[1].len()
            || ranges[2].len() != ranges[3].len()
        {
            warn!("rpmi-shmem-mbox: invalid queue region size");
            return;
        }

        let queues = core::array::from_fn(|index| {
            let range = &ranges[index];
            let base = range.start as *mut u8;
            let num_slots = range.len().saturating_sub(headers_size) / slot_size;
            // SAFETY: the compatible node supplies shared queue memory, and
            // the size and alignment checks above cover every queue slot.
            unsafe {
                SmqQueue::new(
                    base.cast(),
                    base.add(slot_size).cast(),
                    base.add(headers_size),
                    slot_size,
                    num_slots,
                )
            }
        });
        let doorbell_base = ranges[4].start;
        if doorbell_base % core::mem::align_of::<Le32>() != 0
            || ranges[4].len() < MAILBOX_INT_EN_REG_OFFSET + core::mem::size_of::<Le32>()
        {
            warn!("rpmi-shmem-mbox: invalid doorbell region");
            return;
        }
        let Some(interrupt_enable) = doorbell_base
            .checked_add(MAILBOX_INT_EN_REG_OFFSET)
            .map(|address| address as *const Le32)
        else {
            warn!("rpmi-shmem-mbox: invalid doorbell address");
            return;
        };
        // SAFETY: the doorbell region bounds and alignment were validated.
        unsafe { (*interrupt_enable).write(1) };
        let Some(doorbell) = doorbell_base
            .checked_add(MAILBOX_DOORBELL_TRIGGER_OFFSET)
            .map(|address| address as *const Le32)
        else {
            warn!("rpmi-shmem-mbox: invalid doorbell address");
            return;
        };
        // SAFETY: every queue has aligned headers and at least two complete
        // slots, and the doorbell lies inside its FDT register region.
        let transport = unsafe {
            RpmiMailbox::new(
                slot_size,
                timebase_frequency as u64,
                queues,
                Some(&*doorbell),
            )
        };
        mailbox = Some(Box::leak(Box::new(transport)) as &'static RpmiMailbox);
    });
    mailbox
}

/// Read the hart's mtime tick count via the TIME CSR.
///
/// The TIME CSR is readable in M-mode (the SBI ecall path) and, unlike
/// `mcycle` (`mcountinhibit.CY`), cannot be inhibited.
#[inline]
fn mtime_ticks() -> u64 {
    riscv::register::time::read64()
}

/// RPMI service-group identifiers used by Prototyper services.
pub mod servicegroup {
    /// Base service group.
    pub const BASE: u16 = ::rpmi::base::SERVICE_GROUP_ID;
    /// System reset service group.
    pub const SYSTEM_RESET: u16 = ::rpmi::system_reset::SERVICE_GROUP_ID;
    /// CPPC service group.
    pub const CPPC: u16 = ::rpmi::cppc::SERVICE_GROUP_ID;
    /// Voltage service group.
    pub const VOLTAGE: u16 = ::rpmi::voltage::SERVICE_GROUP_ID;
    /// Clock service group.
    pub const CLOCK: u16 = ::rpmi::clock::SERVICE_GROUP_ID;
    /// Device power domain service group.
    pub const DOMAIN: u16 = ::rpmi::device_power::SERVICE_GROUP_ID;
}

/// CPPC register-probe request for the K3 management firmware.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CppcProbeReq {
    /// Hart identifier.
    pub hart_id: u32,
    /// CPPC register identifier.
    pub reg_id: u32,
}

/// CPPC register-read request for the K3 management firmware.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CppcReadReq {
    /// Hart identifier.
    pub hart_id: u32,
    /// CPPC register identifier.
    pub reg_id: u32,
}

/// CPPC register-write request for the K3 management firmware.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CppcWriteReq {
    /// Hart identifier.
    pub hart_id: u32,
    /// CPPC register identifier.
    pub reg_id: u32,
    /// Lower 32 bits of the register value.
    pub data_lo: u32,
    /// Upper 32 bits of the register value.
    pub data_hi: u32,
}

impl CppcProbeReq {
    pub fn to_bytes(self) -> [u8; 8] {
        let mut bytes = [0; 8];
        bytes[..4].copy_from_slice(&self.hart_id.to_le_bytes());
        bytes[4..].copy_from_slice(&self.reg_id.to_le_bytes());
        bytes
    }
}

impl CppcReadReq {
    pub fn to_bytes(self) -> [u8; 8] {
        let mut bytes = [0; 8];
        bytes[..4].copy_from_slice(&self.hart_id.to_le_bytes());
        bytes[4..].copy_from_slice(&self.reg_id.to_le_bytes());
        bytes
    }
}

impl CppcWriteReq {
    pub fn to_bytes(self) -> [u8; 16] {
        let mut bytes = [0; 16];
        bytes[..4].copy_from_slice(&self.hart_id.to_le_bytes());
        bytes[4..8].copy_from_slice(&self.reg_id.to_le_bytes());
        bytes[8..12].copy_from_slice(&self.data_lo.to_le_bytes());
        bytes[12..].copy_from_slice(&self.data_hi.to_le_bytes());
        bytes
    }
}

#[repr(transparent)]
struct Le32(UnsafeCell<u32>);

// SAFETY: accesses are volatile and ordered by the shared-memory queue
// protocol; remote writes are expected while shared references exist.
unsafe impl Send for Le32 {}
// SAFETY: see the `Send` implementation above.
unsafe impl Sync for Le32 {}

impl Le32 {
    #[inline]
    fn read(&self) -> u32 {
        // SAFETY: `self` aliases a shared-memory word; the volatile read
        // cannot be cached or reordered by the compiler.
        unsafe { u32::from_le(self.0.get().read_volatile()) }
    }

    #[inline]
    fn write(&self, value: u32) {
        // SAFETY: `self` aliases a shared-memory word; the volatile write
        // cannot be elided or reordered by the compiler.
        unsafe { self.0.get().write_volatile(value.to_le()) }
    }
}

struct SmqQueue {
    head: *const Le32,
    tail: *const Le32,
    buffer: *mut u8,
    slot_size: usize,
    num_slots: usize,
}

// SAFETY: the queue aliases shared memory accessed by both the AP and the
// PuC; all accesses are volatile and guarded by the queue indices published
// with release/acquire fences, so sharing the queue between harts (and the
// dispatcher static) is sound.
unsafe impl Send for SmqQueue {}
unsafe impl Sync for SmqQueue {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum QueueError {
    Full,
    Invalid,
}

impl SmqQueue {
    /// Creates a queue view.
    ///
    /// # Safety
    ///
    /// `head`, `tail` and `buffer` must point into shared memory that is
    /// accessible to both the AP and the PuC, and `slot_size * num_slots`
    /// bytes must be readable/writable at `buffer`. `slot_size` must hold an
    /// RPMI header and `num_slots` must be at least two.
    const unsafe fn new(
        head: *const Le32,
        tail: *const Le32,
        buffer: *mut u8,
        slot_size: usize,
        num_slots: usize,
    ) -> Self {
        Self {
            head,
            tail,
            buffer,
            slot_size,
            num_slots,
        }
    }

    fn indices(&self) -> Option<(usize, usize)> {
        // SAFETY: `new` requires both indices to remain valid for this queue.
        let head = unsafe { &*self.head }.read() as usize;
        let tail = unsafe { &*self.tail }.read() as usize;
        (head < self.num_slots && tail < self.num_slots).then_some((head, tail))
    }

    /// SpacemiT K3 L1 data-cache line size.
    const CACHE_LINE_SIZE: usize = 64;

    /// Clean and invalidate a data-cache range so writes become visible to
    /// the remote management processor.
    ///
    /// # Safety
    ///
    /// The range must identify shared memory accessible to the current hart.
    unsafe fn dcache_clean_invalid_range(addr: usize, size: usize) {
        // SAFETY: the caller guarantees the cache-block range is accessible.
        unsafe {
            core::arch::asm!("fence rw, rw");
            let start = addr & !(Self::CACHE_LINE_SIZE - 1);
            let end = addr + size;
            let mut op = start;
            while op < end {
                core::arch::asm!("cbo.flush 0({})", in(reg) op);
                op += Self::CACHE_LINE_SIZE;
            }
            core::arch::asm!("fence rw, rw");
        }
    }

    /// Invalidate a data-cache range so the local hart reads data written by
    /// the remote management processor.
    ///
    /// # Safety
    ///
    /// The range must identify shared memory accessible to the current hart.
    unsafe fn dcache_invalid_range(addr: usize, size: usize) {
        // SAFETY: the caller guarantees the cache-block range is accessible.
        unsafe {
            core::arch::asm!("fence rw, rw");
            let start = addr & !(Self::CACHE_LINE_SIZE - 1);
            let end = addr + size;
            let mut op = start;
            while op < end {
                core::arch::asm!("cbo.inval 0({})", in(reg) op);
                op += Self::CACHE_LINE_SIZE;
            }
            core::arch::asm!("fence rw, rw");
        }
    }

    /// # Safety
    ///
    /// `data.len()` must not exceed `slot_size - 8`.
    unsafe fn send(
        &self,
        header: &MessageHeader,
        data: &[u8],
        doorbell: Option<&Le32>,
    ) -> Result<(), QueueError> {
        // SAFETY: `new` requires the queue indices to remain in shared memory.
        unsafe {
            Self::dcache_invalid_range(self.head as usize, core::mem::size_of::<Le32>());
            Self::dcache_invalid_range(self.tail as usize, core::mem::size_of::<Le32>());
        }
        let Some((head, tail)) = self.indices() else {
            return Err(QueueError::Invalid);
        };
        if (tail + 1) % self.num_slots == head {
            return Err(QueueError::Full);
        }
        if data.len() > self.slot_size - RPMI_MSG_HDR_SIZE
            || header.data_len() as usize != data.len()
        {
            return Err(QueueError::Invalid);
        }

        // SAFETY: the validated tail index selects a complete queue slot.
        let slot = unsafe { self.buffer.add(tail * self.slot_size) };

        // Write the header little-endian. The RPMI logical words pack into
        // the same byte layout the shared-memory transport specifies.
        let words = header.words();
        // SAFETY: RPMI slots are aligned to `slot_size`, which is at least 64.
        unsafe {
            (slot as *mut u32).write_volatile(words[0].to_le());
            (slot.add(4) as *mut u32).write_volatile(words[1].to_le());
        }
        if !data.is_empty() {
            // SAFETY: the length check keeps the copy inside this slot.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    slot.add(RPMI_MSG_HDR_SIZE),
                    data.len(),
                );
            }
        }

        // SAFETY: `slot` lies in the shared buffer required by `new`.
        unsafe { Self::dcache_clean_invalid_range(slot as usize, self.slot_size) };

        // SAFETY: `new` requires the tail index to remain valid.
        unsafe { &*self.tail }.write(((tail + 1) % self.num_slots) as u32);
        // SAFETY: `new` requires the tail index to remain in shared memory.
        unsafe {
            Self::dcache_clean_invalid_range(self.tail as usize, core::mem::size_of::<Le32>())
        };

        if let Some(db) = doorbell {
            db.write(1);
        }
        Ok(())
    }

    /// # Safety
    ///
    /// The shared-memory regions passed to `new` must remain valid.
    unsafe fn receive(
        &self,
        service_group_id: u16,
        service_id: u8,
        token: u16,
        out: &mut [u8],
    ) -> Result<Option<usize>, QueueError> {
        // SAFETY: `new` requires the queue indices to remain in shared memory.
        unsafe {
            Self::dcache_invalid_range(self.head as usize, core::mem::size_of::<Le32>());
            Self::dcache_invalid_range(self.tail as usize, core::mem::size_of::<Le32>());
        }
        let Some((head, tail)) = self.indices() else {
            return Err(QueueError::Invalid);
        };
        if head == tail {
            return Ok(None);
        }

        let mut pos = head;
        loop {
            // SAFETY: `pos` is reduced modulo the validated slot count.
            let slot = unsafe { self.buffer.add(pos * self.slot_size) };
            // SAFETY: `slot` lies in the shared buffer required by `new`.
            unsafe { Self::dcache_invalid_range(slot as usize, self.slot_size) };
            // SAFETY: RPMI slots are aligned and contain an eight-byte header.
            let slot_token = unsafe { (slot.add(6) as *const u16).read_volatile() };
            if u16::from_le(slot_token) == token {
                break;
            }
            pos = (pos + 1) % self.num_slots;
            if pos == tail {
                return Ok(None);
            }
        }

        if pos != head {
            // SAFETY: both indices were validated against the slot count.
            let head_slot = unsafe { self.buffer.add(head * self.slot_size) };
            let pos_slot = unsafe { self.buffer.add(pos * self.slot_size) };
            // SAFETY: both slots lie in the shared buffer required by `new`.
            unsafe {
                for i in 0..self.slot_size {
                    let a = head_slot.add(i).read_volatile();
                    let b = pos_slot.add(i).read_volatile();
                    head_slot.add(i).write_volatile(b);
                    pos_slot.add(i).write_volatile(a);
                }
                Self::dcache_clean_invalid_range(head_slot as usize, self.slot_size);
                Self::dcache_clean_invalid_range(pos_slot as usize, self.slot_size);
            }
        }

        // SAFETY: the validated head index selects a complete queue slot.
        let slot = unsafe { self.buffer.add(head * self.slot_size) };
        // SAFETY: RPMI slots are aligned and contain an eight-byte header.
        let header = MessageHeader::from_words(unsafe {
            [
                u32::from_le((slot as *const u32).read_volatile()),
                u32::from_le((slot.add(4) as *const u32).read_volatile()),
            ]
        });
        let datalen = header.data_len() as usize;
        let valid = MessageHeader::new(
            header.service_group_id(),
            header.service_id(),
            header.flags(),
            header.data_len(),
            header.token(),
        ) == Some(header)
            && header.service_group_id() == service_group_id
            && header.service_id() == service_id
            && header.message_type() == Ok(MessageType::Acknowledgement)
            && datalen <= self.slot_size - RPMI_MSG_HDR_SIZE
            && datalen <= out.len();
        if valid && datalen > 0 {
            // SAFETY: header validation bounds the copy by the slot and output.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    slot.add(RPMI_MSG_HDR_SIZE),
                    out.as_mut_ptr(),
                    datalen,
                );
            }
        }

        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        // SAFETY: `new` requires the head index to remain valid.
        unsafe { &*self.head }.write(((head + 1) % self.num_slots) as u32);
        // SAFETY: `new` requires the head index to remain in shared memory.
        unsafe {
            Self::dcache_clean_invalid_range(self.head as usize, core::mem::size_of::<Le32>())
        };
        if valid {
            Ok(Some(datalen))
        } else {
            Err(QueueError::Invalid)
        }
    }
}

/// RPMI shared-memory queue indices.
pub mod queue_idx {
    /// AP to PuC request.
    pub const A2P_REQ: usize = 0;
    /// PuC to AP acknowledgement.
    pub const P2A_ACK: usize = 1;
    /// Number of queues.
    pub const MAX_COUNT: usize = 4;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailboxError {
    /// The request or response could not be transferred correctly.
    Io,
    /// The response did not arrive before the transport deadline.
    Timeout,
}

/// RPMI protocol attributes reported by the Base service group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RpmiProtocolInfo {
    /// Implemented RPMI specification version.
    pub spec_version: u32,
    /// RPMI implementation identifier.
    pub implementation_id: u32,
    /// RPMI implementation version.
    pub implementation_version: u32,
}

/// RPMI shared-memory mailbox controller.
pub struct RpmiMailbox {
    queues: [SmqQueue; queue_idx::MAX_COUNT],
    doorbell: Option<&'static Le32>,
    timebase_frequency: u64,
    next_token: AtomicU16,
    operation: Mutex<()>,
}

impl RpmiMailbox {
    /// Creates a mailbox controller.
    ///
    /// # Safety
    ///
    /// Every queue must alias shared memory accessible to both the AP and
    /// the PuC (see [`SmqQueue::new`]), use `slot_size`, and contain at least
    /// two slots. `timebase_frequency` must be nonzero.
    unsafe fn new(
        slot_size: usize,
        timebase_frequency: u64,
        queues: [SmqQueue; queue_idx::MAX_COUNT],
        doorbell: Option<&'static Le32>,
    ) -> Self {
        debug_assert!(
            queues
                .iter()
                .all(|queue| queue.slot_size == slot_size && queue.num_slots >= 2)
        );
        Self {
            queues,
            doorbell,
            timebase_frequency,
            next_token: AtomicU16::new(1),
            operation: Mutex::new(()),
        }
    }

    fn alloc_token(&self) -> u16 {
        let mut token = self.next_token.load(Ordering::Relaxed);
        loop {
            let next = token.wrapping_add(1).max(1);
            match self.next_token.compare_exchange_weak(
                token,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return next,
                Err(current) => token = current,
            }
        }
    }

    /// Maximum RPMI message-data length supported by the shared-memory slots.
    pub fn message_data_max_len(&self) -> usize {
        self.queues[queue_idx::A2P_REQ].slot_size - RPMI_MSG_HDR_SIZE
    }

    fn base_word(&self, service_id: u8, request: &[u8]) -> Option<u32> {
        let mut response = [0u8; 8];
        match self.normal_request_with_status(
            servicegroup::BASE,
            service_id,
            request,
            &mut response,
        ) {
            Ok((Status::Success, 8)) => Some(u32::from_le_bytes([
                response[4],
                response[5],
                response[6],
                response[7],
            ])),
            _ => None,
        }
    }

    /// Read the RPMI specification and implementation attributes.
    pub fn protocol_info(&self) -> Option<RpmiProtocolInfo> {
        Some(RpmiProtocolInfo {
            spec_version: self.base_word(::rpmi::base::GET_SPEC_VERSION, &[])?,
            implementation_id: self.base_word(::rpmi::base::GET_IMPLEMENTATION_ID, &[])?,
            implementation_version: self
                .base_word(::rpmi::base::GET_IMPLEMENTATION_VERSION, &[])?,
        })
    }

    /// Probe an RPMI service group and return its nonzero version.
    pub fn probe_service_group(&self, service_group_id: u16) -> Option<u32> {
        let request = (service_group_id as u32).to_le_bytes();
        self.base_word(::rpmi::base::PROBE_SERVICE_GROUP, &request)
            .filter(|version| *version != 0)
    }

    fn send_request(&self, header: &MessageHeader, request: &[u8]) -> Result<(), MailboxError> {
        let deadline = mtime_ticks()
            .saturating_add(RPMI_DEF_TX_TIMEOUT as u64 * self.timebase_frequency / 1_000);
        loop {
            // SAFETY: the queue aliases the shared memory established by `new`.
            match unsafe { self.queues[queue_idx::A2P_REQ].send(header, request, self.doorbell) } {
                Ok(()) => return Ok(()),
                Err(QueueError::Invalid) => return Err(MailboxError::Io),
                Err(QueueError::Full) if mtime_ticks() >= deadline => {
                    return Err(MailboxError::Timeout);
                }
                Err(QueueError::Full) => core::hint::spin_loop(),
            }
        }
    }

    /// Perform a normal RPMI request expecting a response.
    ///
    /// Sends a `NormalRequest` message on the A2P_REQ queue and waits for
    /// the matching acknowledgement on the P2A_ACK queue. The response
    /// payload is copied into `resp` and the first word (the status code)
    /// is returned as a [`Status`].
    ///
    /// Returns an error when the request cannot be sent or the response does
    /// not arrive before the transport deadline.
    pub fn normal_request_with_status(
        &self,
        servicegroup_id: u16,
        service_id: u8,
        req: &[u8],
        resp: &mut [u8],
    ) -> Result<(Status, usize), MailboxError> {
        let _operation = self.operation.lock();
        if resp.len() < 4 {
            return Err(MailboxError::Io);
        }
        let token = self.alloc_token();
        let data_len = u16::try_from(req.len()).map_err(|_| MailboxError::Io)?;
        let header = MessageHeader::new(
            servicegroup_id,
            service_id,
            MessageType::NormalRequest.bits(),
            data_len,
            token,
        )
        .ok_or(MailboxError::Io)?;
        self.send_request(&header, req)?;
        let deadline = mtime_ticks()
            .saturating_add(RPMI_DEF_RX_TIMEOUT as u64 * self.timebase_frequency / 1_000);
        let mut rx = vec![0u8; self.message_data_max_len()];
        loop {
            // SAFETY: the queue aliases the shared memory established by `new`.
            let received = unsafe {
                self.queues[queue_idx::P2A_ACK].receive(servicegroup_id, service_id, token, &mut rx)
            };
            if let Some(n) = received.map_err(|_| MailboxError::Io)? {
                if n < 4 {
                    return Err(MailboxError::Io);
                }
                if n > resp.len() {
                    return Err(MailboxError::Io);
                }
                let status = Status::try_from(u32::from_le_bytes([rx[0], rx[1], rx[2], rx[3]]))
                    .unwrap_or(Status::Failed);
                resp[..n].copy_from_slice(&rx[..n]);
                return Ok((status, n));
            }
            if mtime_ticks() >= deadline {
                break;
            }
            // No message yet: yield and retry.
            core::hint::spin_loop();
        }
        warn!(
            "RPMI send TIMEOUT: sg={} svc={} token={} (no ack in {} ms)",
            servicegroup_id, service_id, token, RPMI_DEF_RX_TIMEOUT
        );
        Err(MailboxError::Timeout)
    }

    /// Perform a posted RPMI request without a response.
    ///
    /// Sends a `PostedRequest` message on the A2P_REQ queue and returns
    /// immediately.
    pub fn posted_request(
        &self,
        servicegroup_id: u16,
        service_id: u8,
        req: &[u8],
    ) -> Result<(), MailboxError> {
        let _operation = self.operation.lock();
        let token = self.alloc_token();
        let data_len = u16::try_from(req.len()).map_err(|_| MailboxError::Io)?;
        let header = MessageHeader::new(
            servicegroup_id,
            service_id,
            MessageType::PostedRequest.bits(),
            data_len,
            token,
        )
        .ok_or(MailboxError::Io)?;
        self.send_request(&header, req)
    }
}
