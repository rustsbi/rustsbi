//! Pending firmware IPI work, posted by senders and consumed by one target hart.
//!
//! Each kind occupies one bit, so repeated posts of the same kind coalesce. The
//! consumer removes one selected kind at a time, leaving every other pending
//! kind for a later pop.

#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicU8, Ordering};

/// One kind of firmware work pending on a target hart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IpiEvent {
    /// Notify the supervisor by raising its software interrupt.
    Supervisor,
    /// Run the local remote-fence handler.
    Fence,
}

impl IpiEvent {
    const fn bit(self) -> u8 {
        match self {
            Self::Supervisor => 1 << 0,
            Self::Fence => 1 << 1,
        }
    }
}

/// Set of firmware IPI work kinds pending on one hart.
///
/// Each kind occupies one bit. Atomics exist only for integer types, so this
/// set is the typed value stored in [`AtomicU8`]; callers work with
/// [`IpiEvent`] values rather than raw bits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingEvents(u8);

impl PendingEvents {
    const EMPTY: Self = Self(0);

    const fn contains(self, event: IpiEvent) -> bool {
        self.0 & event.bit() != 0
    }

    /// Returns the next event to consume, preferring supervisor notification.
    const fn next(self) -> Option<IpiEvent> {
        if self.contains(IpiEvent::Supervisor) {
            Some(IpiEvent::Supervisor)
        } else if self.contains(IpiEvent::Fence) {
            Some(IpiEvent::Fence)
        } else {
            None
        }
    }
}

/// IPI work posted by senders and consumed by one target hart.
pub(crate) struct PendingIpis {
    events: AtomicU8,
}

impl PendingIpis {
    pub(crate) const fn new() -> Self {
        Self {
            events: AtomicU8::new(PendingEvents::EMPTY.0),
        }
    }

    /// Posts one event, coalescing with an already pending event of that kind.
    pub(crate) fn insert(&self, event: IpiEvent) {
        self.events.fetch_or(event.bit(), Ordering::Release);
    }

    /// Removes and returns one pending event without disturbing the others.
    ///
    /// Supervisor events are preferred, so a combined post notifies the
    /// supervisor on this pop and leaves the fence event for the next one.
    pub(crate) fn pop_pending(&self) -> Option<IpiEvent> {
        loop {
            let pending = PendingEvents(self.events.load(Ordering::Acquire));
            let event = pending.next()?;
            if self.take(event) {
                return Some(event);
            }
            // A concurrent consumer removed the selected kind after our load.
            // Retry so a kind posted in the meantime is still observed.
        }
    }

    fn take(&self, event: IpiEvent) -> bool {
        // Clear only the selected kind. Other kinds may have been posted
        // since pop_pending read its snapshot.
        PendingEvents(self.events.fetch_and(!event.bit(), Ordering::AcqRel)).contains(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consuming_an_event_preserves_a_different_concurrent_post() {
        for (first, posted) in [
            (IpiEvent::Supervisor, IpiEvent::Fence),
            (IpiEvent::Fence, IpiEvent::Supervisor),
        ] {
            let pending = PendingIpis::new();
            pending.insert(first);
            let snapshot = PendingEvents(pending.events.load(Ordering::Acquire));
            let selected = snapshot.next().unwrap();
            // A sender posts another kind after the receiver selects its event.
            pending.insert(posted);
            assert!(pending.take(selected));
            assert_eq!(pending.pop_pending(), Some(posted));
            assert_eq!(pending.pop_pending(), None);
        }
    }

    #[test]
    fn duplicate_posts_coalesce_and_supervisor_is_consumed_first() {
        let pending = PendingIpis::new();
        pending.insert(IpiEvent::Fence);
        pending.insert(IpiEvent::Supervisor);
        pending.insert(IpiEvent::Fence);
        assert_eq!(pending.pop_pending(), Some(IpiEvent::Supervisor));
        assert_eq!(pending.pop_pending(), Some(IpiEvent::Fence));
        assert_eq!(pending.pop_pending(), None);
    }
}
