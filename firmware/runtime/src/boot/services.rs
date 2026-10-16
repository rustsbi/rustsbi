//! Runtime borrows from the permanently published SBI owners.

use crate::{Error, Result, fwft::Fwft, ipi::Ipi, pmu::Pmu, timer::Timer};
use spin::Once;

/// Stable borrows registered after publishing the SBI dispatcher.
pub struct Services {
    /// Supervisor timer owner.
    pub timer: &'static Timer,
    /// Firmware IPI owner.
    pub ipi: &'static Ipi,
    /// Machine-counter owner.
    pub pmu: Option<&'static Pmu>,
    /// Firmware-feature owner.
    pub fwft: Option<&'static Fwft>,
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

pub(crate) fn ipi() -> Option<&'static Ipi> {
    SERVICES.get().map(|services| services.ipi)
}

pub(crate) fn pmu() -> Option<&'static Pmu> {
    SERVICES.get().and_then(|services| services.pmu)
}
pub(crate) fn fwft() -> Option<&'static Fwft> {
    SERVICES.get().and_then(|services| services.fwft)
}
