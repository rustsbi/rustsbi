//! Optional hardware wakeup for harts that have not entered firmware.

use crate::{Error, Result, ipi};
use alloc::boxed::Box;
use spin::Once;

use super::HartId;

/// A platform's hardware hart-release device.
pub trait HartWake: Send + Sync {
    /// Requests hardware wakeup for `hart`.
    ///
    /// Returns `true` if the request was sent and `false` if Runtime should use an IPI.
    fn wake(&self, hart: HartId) -> Result<bool>;
}

static WAKEUP: Once<Box<dyn HartWake>> = Once::new();

/// Publishes a hardware wakeup device before harts can be started.
///
/// Only the first registration takes effect.
pub fn install_wakeup(device: impl HartWake + 'static) {
    WAKEUP.call_once(|| Box::new(device));
}

/// Returns whether a hardware wakeup device has been installed.
pub fn wakeup_installed() -> bool {
    WAKEUP.get().is_some()
}

pub(super) fn wake(hart: HartId) -> Result<()> {
    if let Some(device) = WAKEUP.get()
        && device.wake(hart)?
    {
        return Ok(());
    }
    ipi::get().ok_or(Error::IoError)?.send_hardware_ipi(hart)
}
