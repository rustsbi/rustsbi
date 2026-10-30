//! Runtime borrows from the permanently published SBI owners.

use crate::{Error, Result, timer::Timer};
use spin::Once;

/// Stable borrows registered after publishing the SBI dispatcher.
pub struct Services {
    /// Supervisor timer owner.
    pub timer: &'static Timer,
}

static SERVICES: Once<Services> = Once::new();

/// Registers services borrowed from their permanently stored owner.
/// Call once before configuring harts or enabling machine interrupt sources.
/// Duplicate registration preserves the original services.
pub fn install_services(services: Services) -> Result<()> {
    let mut installed = false;
    SERVICES.call_once(|| {
        installed = true;
        services
    });
    if installed {
        Ok(())
    } else {
        Err(Error::AlreadyInitialized)
    }
}

pub(crate) fn timer() -> Option<&'static Timer> {
    SERVICES.get().map(|services| services.timer)
}


