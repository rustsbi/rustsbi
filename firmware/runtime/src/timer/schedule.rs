//! Deadline selection for the machine timer comparator.

/// A deadline served by the machine timer comparator.
#[derive(Clone, Copy)]
pub(super) enum TimerTarget {
    Supervisor,
    Firmware,
}

/// The deadlines that have expired during one comparator interrupt.
#[derive(Default)]
pub(super) struct Expired {
    pub(super) supervisor: bool,
    pub(super) firmware: bool,
}

/// Per-hart deadlines sharing one comparator.
#[derive(Default)]
pub(super) struct Schedule {
    supervisor: Option<u64>,
    firmware: Option<u64>,
}

impl Schedule {
    /// Replaces or cancels a target's deadline and returns the earliest one.
    ///
    /// `u64::MAX` is the hardware cancellation value and is treated as absent.
    pub(super) fn set(&mut self, target: TimerTarget, deadline: Option<u64>) -> Option<u64> {
        let deadline = deadline.filter(|&value| value != u64::MAX);
        match target {
            TimerTarget::Supervisor => self.supervisor = deadline,
            TimerTarget::Firmware => self.firmware = deadline,
        }
        self.next()
    }

    /// Returns the earliest active deadline.
    pub(super) fn next(&self) -> Option<u64> {
        match (self.supervisor, self.firmware) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(deadline), None) | (None, Some(deadline)) => Some(deadline),
            (None, None) => None,
        }
    }

    /// Removes and returns all deadlines at or before `now`.
    pub(super) fn take_expired(&mut self, now: u64) -> Expired {
        let mut expired = Expired::default();
        if self.supervisor.is_some_and(|deadline| deadline <= now) {
            self.supervisor = None;
            expired.supervisor = true;
        }
        if self.firmware.is_some_and(|deadline| deadline <= now) {
            self.firmware = None;
            expired.firmware = true;
        }
        expired
    }
}
