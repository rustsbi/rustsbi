//! Optional hardware wakeup for harts that have not entered firmware.

use crate::{Error, Result, ipi};
use alloc::boxed::Box;
use spin::Once;

use super::HartId;

/// A platform's hardware hart-release device.
pub trait HartWake: Send + Sync {
    /// Returns `true` if hardware wakeup was requested, or `false` to use an IPI.
    fn wake(&self, hart: HartId) -> Result<bool>;
}

static WAKEUP: Once<Box<dyn HartWake>> = Once::new();

/// Publishes the optional hardware wakeup device before harts can be started.
///
/// Only the first installed device is retained; later calls are ignored once a
/// device is installed.
pub fn install_wakeup(device: Option<Box<dyn HartWake>>) {
    if let Some(device) = device {
        WAKEUP.call_once(|| device);
    }
}

pub(super) fn wake(hart: HartId) -> Result<()> {
    if let Some(device) = WAKEUP.get()
        && device.wake(hart)?
    {
        return Ok(());
    }
    ipi::get().ok_or(Error::IoError)?.send_hardware_ipi(hart)
}
