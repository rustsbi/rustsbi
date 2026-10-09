//! Optional hardware wakeup for harts that have not entered firmware.

use spin::Once;

use super::HartId;

/// A platform's hardware hart-release device.
pub trait HartWake: Send + Sync {
    /// Requests hardware wakeup, returning `false` to use an IPI instead.
    ///
    /// An error aborts the start request; Runtime does not then fall back to an IPI.
    fn wake(&self, hart: HartId) -> crate::Result<bool>;
}

static WAKEUP: Once<Option<&'static dyn HartWake>> = Once::new();

/// Publishes the optional hardware wakeup device before harts can be started.
pub fn install_wakeup(device: Option<&'static dyn HartWake>) -> crate::Result<()> {
    let mut installed = false;
    WAKEUP.call_once(|| {
        installed = true;
        device
    });
    if installed {
        Ok(())
    } else {
        Err(crate::Error::AlreadyInitialized)
    }
}

pub(super) fn wake(hart: HartId) -> Result<(), super::StartError> {
    let wakeup = WAKEUP.get().ok_or(super::StartError::WakeFailed)?;
    if let Some(device) = *wakeup
        && device
            .wake(hart)
            .map_err(|_| super::StartError::WakeFailed)?
    {
        return Ok(());
    }
    crate::ipi::Ipi::current()
        .map_err(|_| super::StartError::WakeFailed)?
        .send(hart)
        .map_err(|_| super::StartError::WakeFailed)
}
