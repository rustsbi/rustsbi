//! Fair lock used by the hart protocol state.

use core::cell::UnsafeCell;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicUsize, Ordering};
pub(super) struct TicketLock<T> {
    next: AtomicUsize,
    serving: AtomicUsize,
    value: UnsafeCell<T>,
}

// SAFETY: `value` is accessed only by the unique currently served ticket; the
// ticket release publishes all mutations before the next Acquire observation.
unsafe impl<T: Send> Sync for TicketLock<T> {}

impl<T> TicketLock<T> {
    pub(super) const fn new(value: T) -> Self {
        Self {
            next: AtomicUsize::new(0),
            serving: AtomicUsize::new(0),
            value: UnsafeCell::new(value),
        }
    }

    pub(super) fn lock(&self) -> TicketGuard<'_, T> {
        // Protocol invariant: local notification sources are masked before a
        // ticket is allocated, so an interrupt cannot strand its interrupted
        // context's live ticket while trying to acquire the same lock.
        let interrupt_mask = ProtocolInterruptMask::acquire();
        // `fetch_add` wraps explicitly at the atomic integer modulus. At most
        // one ticket can be live per admitted hart because protocol interrupts
        // are masked, and the configured hart bound is smaller than that
        // modulus. Consequently a wrapped value cannot alias a live ticket.
        let ticket = self.next.fetch_add(1, Ordering::Relaxed);
        while self.serving.load(Ordering::Acquire) != ticket {
            core::hint::spin_loop();
        }
        TicketGuard {
            lock: self,
            ticket,
            interrupt_mask,
        }
    }
}

pub(super) struct TicketGuard<'lock, T> {
    lock: &'lock TicketLock<T>,
    ticket: usize,
    interrupt_mask: ProtocolInterruptMask,
}

impl<T> Deref for TicketGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: this guard owns the unique ticket equal to `serving`.
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for TicketGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        // SAFETY: same unique-ticket proof, plus `&mut self` prevents aliases
        // through this guard.
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for TicketGuard<'_, T> {
    fn drop(&mut self) {
        self.lock
            .serving
            .store(self.ticket.wrapping_add(1), Ordering::Release);
        // Protocol invariant: release precedes restoration, so a newly
        // unmasked local notification can never observe this ticket as held.
        self.interrupt_mask.restore();
    }
}

struct ProtocolInterruptMask {
    previous: usize,
}

impl ProtocolInterruptMask {
    fn acquire() -> Self {
        Self {
            previous: super::instructions::mask_protocol_interrupts(),
        }
    }

    fn restore(&self) {
        super::instructions::restore_protocol_interrupts(self.previous)
    }
}
