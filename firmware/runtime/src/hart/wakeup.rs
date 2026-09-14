//! Optional hardware wakeup for harts that have not entered firmware.

use spin::Once;

use super::HartId;

/// A platform's hardware hart-release device.
pub trait HartWake: Send + Sync {
    /// Returns true if hardware wakeup was requested, or false to use an IPI.
    fn wake(&self, hart: HartId) -> crate::Result<bool>;
}

static WAKEUP: Once<Option<&'static dyn HartWake>> = Once::new();

/// Publishes the optional hardware wakeup device before harts can be started.
pub fn install_wakeup(device: Option<&'static dyn HartWake>) {
    WAKEUP.call_once(|| device);
}

pub(super) fn wake(hart: HartId) -> Result<(), super::StartError> {
    if let Some(device) = WAKEUP.get().copied().flatten() {
        if device
            .wake(hart)
            .map_err(|_| super::StartError::WakeFailed)?
        {
            return Ok(());
        }
    }
    crate::ipi::get()
        .ok_or(super::StartError::WakeFailed)?
        .send(hart)
        .map_err(|_| super::StartError::WakeFailed)
}
