//! Optional hardware wakeup for harts that have not entered firmware.

use crate::{Error, Result};

use super::HartId;

/// A platform's hardware hart-release device.
pub trait HartWake: Send + Sync {
    /// Requests hardware wakeup.
    ///
    /// Returns `true` when the device issued the hardware wakeup request, and
    /// `false` when the hart is already in firmware and needs an IPI.
    fn wake(&self, hart: HartId) -> Result<bool>;
}

pub(super) fn wake(hart: HartId, device: Option<&dyn HartWake>) -> Result<()> {
    if let Some(device) = device
        && device.wake(hart)?
    {
        return Ok(());
    }
    crate::ipi::get()
        .ok_or(Error::IoError)?
        .send(crate::ipi::IpiRequest::for_hart(hart))
}
