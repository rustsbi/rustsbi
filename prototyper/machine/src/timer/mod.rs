//! Timer capability and its private physical-device role.

use alloc::sync::Arc;

mod arch;
mod device;

pub(crate) use device::TimerDevice;

/// Authority to program the current hart's supervisor timer deadline.
///
/// The physical timer address and register convention remain private. This
/// capability is intentionally not `Clone`.
pub struct Timer {
    device: Arc<dyn TimerDevice>,
}

/// Failure while preparing a hart-local timer mechanism.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimerError {
    /// The required architectural timer facility is unavailable.
    Unavailable,
    /// The selected physical hart is outside the validated binding.
    InvalidHart,
}

impl Timer {
    pub(crate) fn new(device: Arc<dyn TimerDevice>) -> Self {
        Self { device }
    }

    /// Programs the current hart's next timer deadline.
    pub fn set_deadline(&self, deadline: u64) {
        self.device.set_compare(arch::current_hart_id(), deadline);
    }

    /// Disables the current hart's timer by selecting the maximal deadline.
    pub fn disable(&self) {
        self.set_deadline(u64::MAX);
    }

    pub(crate) fn trap_device(&self) -> Arc<dyn TimerDevice> {
        Arc::clone(&self.device)
    }
}
