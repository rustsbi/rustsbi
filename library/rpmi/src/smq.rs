//! RPMI Shared Memory Queue (SMQ) transport.
//!
//! The platform management processor (PuC) and the application processor
//! (AP) exchange RPMI messages through four shared-memory ring queues:
//! A2P_REQ (AP→PuC request), P2A_ACK (PuC→AP acknowledgement), P2A_REQ
//! (PuC→AP request) and A2P_ACK (AP→PuC acknowledgement).
//!
//! Each queue is a ring of fixed-size slots. The head index is written by
//! the queue reader and the tail index by the queue writer; all indices and
//! message fields in shared memory are little-endian. The implementation
//! mirrors OpenSBI `lib/utils/mailbox/fdt_mailbox_rpmi_shmem.c`
//! (`__smq_tx` / `__smq_rx`).

use crate::message::MessageHeader;

/// Little-endian volatile 32-bit accessor.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct Le32(pub u32);

impl Le32 {
    /// Read the value in little-endian order.
    #[inline]
    pub fn read(&self) -> u32 {
        // Safety: `self` aliases a shared-memory word; the volatile read
        // cannot be cached or reordered by the compiler.
        unsafe { u32::from_le(core::ptr::addr_of!(self.0).read_volatile()) }
    }

    /// Write the value in little-endian order.
    #[inline]
    pub fn write(&self, value: u32) {
        // Safety: `self` aliases a shared-memory word; the volatile write
        // cannot be elided or reordered by the compiler.
        unsafe {
            core::ptr::addr_of!(self.0)
                .cast_mut()
                .write_volatile(value.to_le())
        }
    }
}

/// A shared-memory ring queue for RPMI messages.
pub struct SmqQueue {
    /// Head (read) index in shared memory.
    head: *const Le32,
    /// Tail (write) index in shared memory.
    tail: *const Le32,
    /// Slot buffer base.
    buffer: *mut u8,
    /// Size of one slot in bytes.
    slot_size: usize,
    /// Number of slots in the ring.
    num_slots: usize,
}

// Safety: the raw pointers make the queue `!Send`/`!Sync` by default, but
// the queue aliases shared memory that is deliberately shared with the PuC.
// The queue itself performs no internal synchronization of the head/tail
// indices (plain volatile reads/writes, no atomic RMW or fences), so sharing
// the queue between harts is sound ONLY when every operation on it is
// serialized by the caller: `RpmiMailbox` holds a spinlock around all
// send/receive calls, giving a single outstanding request per mailbox.
unsafe impl Send for SmqQueue {}
unsafe impl Sync for SmqQueue {}

impl SmqQueue {
    /// Create a new queue view.
    ///
    /// # Safety
    ///
    /// `head`, `tail` and `buffer` must point into shared memory that is
    /// accessible to both the AP and the PuC, and `slot_size * num_slots`
    /// bytes must be readable/writable at `buffer`.
    pub const unsafe fn new(
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

    /// Returns whether the queue is full (`(tail + 1) % n == head`).
    #[inline]
    fn is_full(&self) -> bool {
        let head = unsafe { &*self.head }.read() as usize;
        let tail = unsafe { &*self.tail }.read() as usize;
        (tail + 1) % self.num_slots == head
    }

    /// Returns whether the queue is empty (`head == tail`).
    #[inline]
    fn is_empty(&self) -> bool {
        let head = unsafe { &*self.head }.read() as usize;
        let tail = unsafe { &*self.tail }.read() as usize;
        head == tail
    }

    /// L1 data-cache line size in bytes (64 on the SpacemiT K3, the
    /// reference platform for this crate; adjust per platform).
    ///
    /// Only used by the RISC-V cache operations; kept behind the same
    /// `cfg` so host builds do not warn about dead code.
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    const CACHE_LINE_SIZE: usize = 64;

    /// Clean and invalidate a data-cache range (`cbo.flush`), making writes
    /// visible to the remote management processor (PuC). The instructions
    /// are defined by the RISC-V Zicbom extension; the per-cache-line loop
    /// mirrors the cache helpers of OpenSBI's SpacemiT K1/K3 platform
    /// support (`platform/generic/spacemit/`,
    /// `csi_dcache_clean_invalid_range` / `__DCACHE_CIPA`).
    ///
    /// The `fence`/`cbo.*` instructions are RISC-V-only and `cbo.*` needs
    /// the Zicbom extension, so the operations compile out on other targets
    /// (host builds/tests) and fall back to fences only on RISC-V without
    /// Zicbom.
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    unsafe fn dcache_clean_invalid_range(addr: usize, size: usize) {
        core::arch::asm!("fence rw, rw");
        #[cfg(target_feature = "zicbom")]
        {
            let start = addr & !(Self::CACHE_LINE_SIZE - 1);
            let end = addr + size;
            let mut op = start;
            while op < end {
                core::arch::asm!("cbo.flush 0({})", in(reg) op);
                op += Self::CACHE_LINE_SIZE;
            }
        }
        core::arch::asm!("fence rw, rw");
    }

    /// Host (non-RISC-V) no-op fallback so the crate builds and its unit
    /// tests run on the build host.
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unsafe fn dcache_clean_invalid_range(_addr: usize, _size: usize) {}

    /// Invalidate a data-cache range (`cbo.inval`) so the local hart reads
    /// fresh data written by the remote PuC. The instructions are defined
    /// by the RISC-V Zicbom extension; the per-cache-line loop mirrors the
    /// cache helpers of OpenSBI's SpacemiT K1/K3 platform support
    /// (`csi_dcache_invalid_range` / `__DCACHE_IPA`). Target gating as in
    /// [`SmqQueue::dcache_clean_invalid_range`].
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    unsafe fn dcache_invalid_range(addr: usize, size: usize) {
        core::arch::asm!("fence rw, rw");
        #[cfg(target_feature = "zicbom")]
        {
            let start = addr & !(Self::CACHE_LINE_SIZE - 1);
            let end = addr + size;
            let mut op = start;
            while op < end {
                core::arch::asm!("cbo.inval 0({})", in(reg) op);
                op += Self::CACHE_LINE_SIZE;
            }
        }
        core::arch::asm!("fence rw, rw");
    }

    /// Host (non-RISC-V) no-op fallback so the crate builds and its unit
    /// tests run on the build host.
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unsafe fn dcache_invalid_range(_addr: usize, _size: usize) {}

    /// Enqueue a message into the ring.
    ///
    /// Writes the header and payload into the tail slot, publishes the
    /// little-endian tail index, and rings the optional doorbell register.
    /// Returns `Err(())` when the queue is full.
    ///
    /// # Safety
    ///
    /// `data.len()` must not exceed `slot_size - 8`.
    pub unsafe fn send(
        &self,
        header: &MessageHeader,
        data: &[u8],
        doorbell: Option<&Le32>,
    ) -> Result<(), ()> {
        // Invalidate the PuC-written head (and our own tail) so the freed
        // slots are visible before checking whether the queue is full.
        // Mirrors OpenSBI `__smq_tx`'s `__DCACHE_IPA(headptr/tailptr)`.
        unsafe { Self::dcache_invalid_range(self.head as usize, core::mem::size_of::<Le32>()) };
        unsafe { Self::dcache_invalid_range(self.tail as usize, core::mem::size_of::<Le32>()) };
        if self.is_full() {
            return Err(());
        }
        if data.len() > self.slot_size - crate::message::RPMI_MSG_HDR_SIZE {
            return Err(());
        }

        let tail = unsafe { &*self.tail }.read() as usize;
        let slot = unsafe { self.buffer.add(tail * self.slot_size) };

        // Write header fields little-endian.
        unsafe {
            (slot as *mut u16).write_volatile(header.servicegroup_id.to_le());
            (slot.add(2) as *mut u8).write_volatile(header.service_id);
            (slot.add(3) as *mut u8).write_volatile(header.flags);
            (slot.add(4) as *mut u16).write_volatile(header.datalen.to_le());
            (slot.add(6) as *mut u16).write_volatile(header.token.to_le());
        }
        // Copy payload.
        if !data.is_empty() {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data.as_ptr(),
                    slot.add(crate::message::RPMI_MSG_HDR_SIZE),
                    data.len(),
                );
            }
        }

        // Make the message data visible to the PuC before publishing the tail.
        unsafe { Self::dcache_clean_invalid_range(slot as usize, self.slot_size) };

        // Publish the tail index.
        unsafe { &*self.tail }.write(((tail + 1) % self.num_slots) as u32);
        // Flush the tail index so the PuC sees the new value.
        unsafe {
            Self::dcache_clean_invalid_range(self.tail as usize, core::mem::size_of::<Le32>())
        };

        // Ring the doorbell if present.
        if let Some(db) = doorbell {
            db.write(1);
        }
        Ok(())
    }

    /// Dequeue a message from the ring, matching `token`.
    ///
    /// Scans the queue for the slot carrying `token`, moves it to the head
    /// slot, copies the payload into `out`, and advances the head index.
    /// Returns the number of payload bytes copied, or `Err(())` when no
    /// matching message is present.
    pub unsafe fn receive(&self, token: u16, out: &mut [u8]) -> Result<usize, ()> {
        // Invalidate the PuC-written tail index and slots so we read fresh data.
        unsafe { Self::dcache_invalid_range(self.tail as usize, core::mem::size_of::<Le32>()) };
        if self.is_empty() {
            return Err(());
        }
        let head = unsafe { &*self.head }.read() as usize;
        let tail = unsafe { &*self.tail }.read() as usize;

        // Locate the slot with the matching token.
        let mut pos = head;
        loop {
            let slot = unsafe { self.buffer.add(pos * self.slot_size) };
            unsafe { Self::dcache_invalid_range(slot as usize, self.slot_size) };
            let slot_token = unsafe { (slot.add(6) as *const u16).read_volatile() };
            if u16::from_le(slot_token) == token {
                break;
            }
            pos = (pos + 1) % self.num_slots;
            if pos == tail {
                return Err(());
            }
        }

        // Move the matched message to the head slot if it is not already
        // the first message.
        if pos != head {
            let head_slot = unsafe { self.buffer.add(head * self.slot_size) };
            let pos_slot = unsafe { self.buffer.add(pos * self.slot_size) };
            unsafe {
                for i in 0..self.slot_size {
                    let a = head_slot.add(i).read();
                    let b = pos_slot.add(i).read();
                    head_slot.add(i).write(b);
                    pos_slot.add(i).write(a);
                }
            }
        }

        // Read header and payload from the head slot.
        let slot = unsafe { self.buffer.add(head * self.slot_size) };
        let datalen = unsafe { u16::from_le((slot.add(4) as *const u16).read_volatile()) } as usize;
        // `datalen` is written by the remote PuC and must not be trusted:
        // reject payloads that do not fit the slot (mirrors OpenSBI
        // `__smq_rx` sanity checks) instead of reading past the ring.
        if datalen > self.slot_size - crate::message::RPMI_MSG_HDR_SIZE {
            return Err(());
        }
        let n = datalen.min(out.len());
        if n > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    slot.add(crate::message::RPMI_MSG_HDR_SIZE),
                    out.as_mut_ptr(),
                    n,
                );
            }
        }

        // Publish the advanced head index.
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        unsafe { &*self.head }.write(((head + 1) % self.num_slots) as u32);
        // Flush the head index so the PuC sees the freed slot (mirrors
        // OpenSBI `__DCACHE_CIPA(headptr)` in `__smq_rx`).
        unsafe {
            Self::dcache_clean_invalid_range(self.head as usize, core::mem::size_of::<Le32>())
        };
        Ok(n)
    }

    /// Dequeue a notification message from the ring, matching the message
    /// identifier (service group ID + service ID + message type) instead of
    /// a token.
    ///
    /// Notifications carry no token, so they are matched by their message
    /// identifier (mirrors OpenSBI `__smq_rx` with `no_rx_token`). A
    /// `service_id` of `0xff` matches any service within the group (a
    /// convenience extension of this crate; the RPMI spec and OpenSBI
    /// reference implement exact service-id matching). The payload is
    /// copied into `out` and the head index is advanced. Returns the number
    /// of payload bytes copied, or `Err(())` when no matching message is
    /// present.
    pub unsafe fn receive_by_message_id(
        &self,
        servicegroup_id: u16,
        service_id: u8,
        msg_type: u8,
        out: &mut [u8],
    ) -> Result<usize, ()> {
        unsafe { Self::dcache_invalid_range(self.tail as usize, core::mem::size_of::<Le32>()) };
        if self.is_empty() {
            return Err(());
        }
        let head = unsafe { &*self.head }.read() as usize;
        let tail = unsafe { &*self.tail }.read() as usize;

        // Locate the slot whose message identifier matches.
        let mut pos = head;
        loop {
            let slot = unsafe { self.buffer.add(pos * self.slot_size) };
            unsafe { Self::dcache_invalid_range(slot as usize, self.slot_size) };
            let sgid = unsafe { u16::from_le((slot as *const u16).read_volatile()) };
            let sid = unsafe { (slot.add(2) as *const u8).read_volatile() };
            let flags = unsafe { (slot.add(3) as *const u8).read_volatile() };
            let sid_match = service_id == 0xff || sid == service_id; // 0xff: crate wildcard extension
            if sgid == servicegroup_id && sid_match && (flags & 0x7) == msg_type {
                break;
            }
            pos = (pos + 1) % self.num_slots;
            if pos == tail {
                return Err(());
            }
        }

        // Move the matched message to the head slot if it is not already
        // the first message.
        if pos != head {
            let head_slot = unsafe { self.buffer.add(head * self.slot_size) };
            let pos_slot = unsafe { self.buffer.add(pos * self.slot_size) };
            unsafe {
                for i in 0..self.slot_size {
                    let a = head_slot.add(i).read();
                    let b = pos_slot.add(i).read();
                    head_slot.add(i).write(b);
                    pos_slot.add(i).write(a);
                }
            }
        }

        // Read header and payload from the head slot.
        let slot = unsafe { self.buffer.add(head * self.slot_size) };
        let datalen = unsafe { u16::from_le((slot.add(4) as *const u16).read_volatile()) } as usize;
        // `datalen` is written by the remote PuC and must not be trusted:
        // reject payloads that do not fit the slot (mirrors OpenSBI
        // `__smq_rx` sanity checks) instead of reading past the ring.
        if datalen > self.slot_size - crate::message::RPMI_MSG_HDR_SIZE {
            return Err(());
        }
        let n = datalen.min(out.len());
        if n > 0 {
            unsafe {
                core::ptr::copy_nonoverlapping(
                    slot.add(crate::message::RPMI_MSG_HDR_SIZE),
                    out.as_mut_ptr(),
                    n,
                );
            }
        }

        // Publish the advanced head index.
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        unsafe { &*self.head }.write(((head + 1) % self.num_slots) as u32);
        // Flush the head index so the PuC sees the freed slot (mirrors
        // OpenSBI `__DCACHE_CIPA(headptr)` in `__smq_rx`).
        unsafe {
            Self::dcache_clean_invalid_range(self.head as usize, core::mem::size_of::<Le32>())
        };
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::boxed::Box;

    use super::*;
    use crate::message::{MessageType, servicegroup};

    /// Build a queue over a leaked heap buffer so tests run on the host.
    fn test_queue() -> (SmqQueue, &'static mut [Le32]) {
        let buffer = Box::leak(Box::new([0u8; 4 * 64]));
        let indices = Box::leak(Box::new([Le32(0), Le32(0)]));
        let queue = unsafe {
            SmqQueue::new(
                indices.as_ptr(),
                indices.as_ptr().add(1),
                buffer.as_mut_ptr(),
                64,
                4,
            )
        };
        (queue, indices)
    }

    #[test]
    fn test_roundtrip() {
        let (queue, indices) = test_queue();
        let header = MessageHeader::new(
            servicegroup::HSM,
            0x01,
            MessageType::NormalRequest,
            4,
            0x1234,
        );
        let data = [1u8, 2, 3, 4];

        unsafe {
            queue.send(&header, &data, None).unwrap();
        }
        // The sender writes tail; emulate the PuC having consumed nothing.
        assert_eq!(indices[1].read(), 1);

        let mut out = [0u8; 16];
        let n = unsafe { queue.receive(0x1234, &mut out).unwrap() };
        assert_eq!(n, 4);
        assert_eq!(&out[..4], &data);
        assert_eq!(indices[0].read(), 1);
    }

    #[test]
    fn test_token_mismatch() {
        let (queue, _) = test_queue();
        let header = MessageHeader::new(servicegroup::BASE, 0x05, MessageType::NormalRequest, 0, 7);
        unsafe {
            queue.send(&header, &[], None).unwrap();
            assert!(queue.receive(8, &mut [0u8; 4]).is_err());
        }
    }

    #[test]
    fn test_full_queue() {
        let (queue, _) = test_queue();
        for i in 0..3 {
            let header =
                MessageHeader::new(servicegroup::BASE, 0, MessageType::NormalRequest, 0, i);
            unsafe {
                queue.send(&header, &[], None).unwrap();
            }
        }
        let header = MessageHeader::new(servicegroup::BASE, 0, MessageType::NormalRequest, 0, 99);
        unsafe {
            assert!(queue.send(&header, &[], None).is_err());
        }
    }
}
