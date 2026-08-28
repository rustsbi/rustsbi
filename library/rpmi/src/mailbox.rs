//! RPMI mailbox abstraction over shared-memory queues.
//!
//! Provides a mailbox controller that owns the four RPMI queues and the
//! doorbell register, and offers the two typical request patterns defined
//! by OpenSBI `lib/utils/mailbox/rpmi_mailbox.c`:
//! `normal_request_with_status` (request + expected response) and
//! `posted_request` (fire-and-forget).

use core::sync::atomic::{AtomicBool, AtomicU16, Ordering};

use crate::message::{Error, MessageHeader, MessageType, RPMI_MSG_TOKEN_MASK};
use crate::smq::{Le32, SmqQueue};

/// Queue identifiers (fdt_mailbox_rpmi_shmem.c `enum rpmi_queue_idx`).
pub mod queue_idx {
    pub const A2P_REQ: usize = 0;
    pub const P2A_ACK: usize = 1;
    pub const P2A_REQ: usize = 2;
    pub const A2P_ACK: usize = 3;
    pub const MAX_COUNT: usize = 4;
}

/// Upper bound on the number of poll iterations when waiting for the
/// acknowledgement of a normal request.
///
/// This is a spin-iteration budget, not a wall-clock timeout: this crate
/// has no time source in `no_std` context, so on a fast hart 500
/// iterations can elapse in far less than the 500 ms implied by OpenSBI's
/// timer-based `RPMI_DEF_RX_TIMEOUT`. Callers that need a true timeout
/// should poll from a timed context (e.g. the SBI timer) instead of
/// relying on this bound.
pub const RPMI_DEF_RX_TIMEOUT: u32 = 500;

/// Minimal `no_std` spinlock serializing all queue operations.
///
/// The SMQ head/tail indices and slot contents are plain volatile
/// shared-memory values without atomic read-modify-write, so concurrent
/// senders or receivers would race on them. The mailbox therefore
/// serializes every queue access with this lock, mirroring OpenSBI's
/// per-channel lock in `rpmi_mailbox.c`: at most one request is in flight
/// through the mailbox at any time.
struct MailboxLock {
    locked: AtomicBool,
}

impl MailboxLock {
    const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    /// Acquire the lock, spinning until it becomes available.
    fn acquire(&self) -> MailboxGuard<'_> {
        while self
            .locked
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        MailboxGuard { lock: self }
    }
}

/// RAII guard that releases the mailbox lock on drop.
struct MailboxGuard<'a> {
    lock: &'a MailboxLock,
}

impl Drop for MailboxGuard<'_> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

/// RPMI shared-memory mailbox controller.
///
/// Owns the four ring queues and an optional doorbell register used to
/// notify the platform management processor. All operations take `&self`
/// so a single mailbox can be shared between several extensions: a
/// spinlock serializes queue access, so at most one request is in flight
/// through the mailbox at a time.
pub struct RpmiMailbox {
    /// Serializes all queue operations.
    lock: MailboxLock,
    /// Size of one queue slot in bytes.
    slot_size: usize,
    /// The four RPMI queues in `queue_idx` order.
    queues: [SmqQueue; queue_idx::MAX_COUNT],
    /// Optional doorbell register (AP → PuC).
    doorbell: Option<&'static Le32>,
    /// Next token to use for requests.
    next_token: AtomicU16,
}

impl RpmiMailbox {
    /// Create a new mailbox controller.
    ///
    /// # Safety
    ///
    /// Every queue must alias shared memory accessible to both the AP and
    /// the PuC (see [`SmqQueue::new`]).
    pub unsafe fn new(
        slot_size: usize,
        queues: [SmqQueue; queue_idx::MAX_COUNT],
        doorbell: Option<&'static Le32>,
    ) -> Self {
        Self {
            lock: MailboxLock::new(),
            slot_size,
            queues,
            doorbell,
            next_token: AtomicU16::new(1),
        }
    }

    /// Allocate the next message token.
    ///
    /// Token 0 is reserved (invalid). When the counter wraps past
    /// `RPMI_MSG_TOKEN_MASK` (0xffff) the next value would be 0, so token 1
    /// is claimed directly instead of looping forever on an unchanged
    /// value.
    fn alloc_token(&self) -> u16 {
        let mut token = self.next_token.load(Ordering::Relaxed);
        loop {
            let next = token.wrapping_add(1) & RPMI_MSG_TOKEN_MASK;
            let target = if next == 0 { 1 } else { next };
            match self.next_token.compare_exchange_weak(
                token,
                target,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return target,
                Err(current) => token = current,
            }
        }
    }

    /// Perform a normal RPMI request expecting a response.
    ///
    /// Sends a `NormalRequest` message on the A2P_REQ queue and waits for
    /// the matching acknowledgement on the P2A_ACK queue. The response
    /// payload is copied into `resp` and the first word (the status code)
    /// is returned as an [`Error`].
    ///
    /// Returns `Err(())` on transport failure (queue full, timeout).
    ///
    /// Queue operations are serialized by the mailbox spinlock: at most one
    /// request is outstanding through the mailbox at a time (mirroring
    /// OpenSBI's per-channel lock in `rpmi_mailbox.c`).
    pub fn normal_request_with_status(
        &self,
        servicegroup_id: u16,
        service_id: u8,
        req: &[u8],
        resp: &mut [u8],
    ) -> Result<(Error, usize), ()> {
        if resp.len() < 4 {
            return Err(());
        }
        // Serialize the enqueue + ack-wait window against other harts.
        let _guard = self.lock.acquire();
        let token = self.alloc_token();
        let header = MessageHeader::new(
            servicegroup_id,
            service_id,
            MessageType::NormalRequest,
            req.len() as u16,
            token,
        );
        // Safety: the queue aliases shared memory established at `new`.
        unsafe {
            self.queues[queue_idx::A2P_REQ]
                .send(&header, req, self.doorbell)
                .map_err(|_| ())?;
        }
        // Wait for the acknowledgement carrying our token. The ack is
        // received straight into the caller's `resp` (status at offset 0,
        // data from offset 4), mirroring OpenSBI's
        // `rpmi_normal_request_with_status`, which passes `resp` directly
        // as the channel receive buffer: no intermediate buffer is used, so
        // responses are limited only by the caller's `resp` length and the
        // queue slot size.
        for _ in 0..RPMI_DEF_RX_TIMEOUT {
            // Safety: as above.
            let n = unsafe { self.queues[queue_idx::P2A_ACK].receive(token, resp) };
            if let Ok(n) = n {
                let status = i32::from_le_bytes([resp[0], resp[1], resp[2], resp[3]]);
                return Ok((Error::from_status(status), n));
            }
            // No message yet: yield and retry.
            core::hint::spin_loop();
        }
        Err(())
    }

    /// Perform a posted RPMI request without a response.
    ///
    /// Sends a `PostedRequest` message on the A2P_REQ queue and returns
    /// immediately. The enqueue is serialized by the mailbox spinlock.
    pub fn posted_request(
        &self,
        servicegroup_id: u16,
        service_id: u8,
        req: &[u8],
    ) -> Result<(), ()> {
        let _guard = self.lock.acquire();
        let token = self.alloc_token();
        let header = MessageHeader::new(
            servicegroup_id,
            service_id,
            MessageType::PostedRequest,
            req.len() as u16,
            token,
        );
        // Safety: the queue aliases shared memory established at `new`.
        let r = unsafe {
            self.queues[queue_idx::A2P_REQ]
                .send(&header, req, self.doorbell)
                .map_err(|_| ())
        };
        r
    }

    /// Receive an asynchronous notification from the platform management
    /// processor.
    ///
    /// Notifications arrive on the P2A_REQ queue and carry no token; they
    /// are matched by their message identifier (service group ID + service
    /// ID + notification type). The payload is copied into `out`. Returns
    /// the number of payload bytes copied, or `Err(())` when no matching
    /// notification is pending. The queue scan is serialized by the mailbox
    /// spinlock.
    pub fn receive_notification(
        &self,
        servicegroup_id: u16,
        service_id: u8,
        out: &mut [u8],
    ) -> Result<usize, ()> {
        let _guard = self.lock.acquire();
        // Safety: the queue aliases shared memory established at `new`.
        unsafe {
            self.queues[queue_idx::P2A_REQ].receive_by_message_id(
                servicegroup_id,
                service_id,
                MessageType::Notification as u8,
                out,
            )
        }
    }

    /// Returns the slot size of this mailbox.
    pub const fn slot_size(&self) -> usize {
        self.slot_size
    }

    /// Debug helper: the doorbell register address (or 0).
    pub fn doorbell_addr(&self) -> usize {
        self.doorbell.map(|db| db as *const _ as usize).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;
    use std::boxed::Box;

    use super::*;
    use crate::smq::Le32;

    /// Build a mailbox over leaked heap buffers so tests run on the host.
    fn test_mailbox() -> RpmiMailbox {
        let buffer = Box::leak(Box::new([0u8; 4 * 64]));
        let indices = Box::leak(Box::new([Le32(0); 2]));
        let queues = core::array::from_fn(|_| unsafe {
            SmqQueue::new(
                indices.as_ptr(),
                indices.as_ptr().add(1),
                buffer.as_mut_ptr(),
                64,
                4,
            )
        });
        unsafe { RpmiMailbox::new(64, queues, None) }
    }

    #[test]
    fn test_alloc_token_wraps_without_hanging() {
        let mailbox = test_mailbox();
        // Force the counter to the value just before the wrap point: the
        // next allocation must yield 0xffff, and the following one must wrap
        // to token 1 (token 0 is reserved) instead of looping forever on an
        // unchanged value.
        mailbox.next_token.store(0xfffe, Ordering::Relaxed);
        assert_eq!(mailbox.alloc_token(), 0xffff);
        assert_eq!(mailbox.alloc_token(), 1);
        assert_eq!(mailbox.alloc_token(), 2);
    }
}
