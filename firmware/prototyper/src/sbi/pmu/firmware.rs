//! Firmware event identities and their accounting entry.

use super::SbiPmu;
use crate::sbi;
use runtime::{Error, trap::Exception};
pub(crate) use sbi_spec::pmu::FirmwareEvent;

/// Maps an architectural exception to the firmware event it accounts for.
pub(crate) fn from_exception(exception: Exception) -> Option<FirmwareEvent> {
    Some(match exception {
        Exception::IllegalInstruction => FirmwareEvent::IllegalInstruction,
        Exception::LoadMisaligned => FirmwareEvent::MisalignedLoad,
        Exception::StoreMisaligned => FirmwareEvent::MisalignedStore,
        Exception::LoadAccess => FirmwareEvent::AccessLoad,
        Exception::StoreAccess => FirmwareEvent::AccessStore,
        _ => return None,
    })
}

impl SbiPmu {
    /// Accounts for one typed event when the published PMU is ready.
    pub(crate) fn record_event(event: FirmwareEvent) {
        let pmu = sbi::pmu();
        match pmu.with_current(|_, counters| {
            counters.record(event.into());
            Ok(())
        }) {
            Ok(()) | Err(Error::NotReady) => {}
            Err(error) => panic!("PMU event accounting failed: {error}"),
        }
    }
}
