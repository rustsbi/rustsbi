//! Timer protocol adapter.

use machine::Timer as MachineTimer;

pub(super) struct Timer {
    timer: MachineTimer,
}

impl Timer {
    pub(super) fn new(timer: MachineTimer) -> Self {
        Self { timer }
    }
}

impl rustsbi::Timer for Timer {
    fn set_timer(&self, deadline: u64) {
        self.timer.set_deadline(deadline);
    }
}
