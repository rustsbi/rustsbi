//! Runtime's firmware-event accounting service.
//!
//! This is separate from the hardware IPI and timer services.
//! The concrete PMU accounting policy remains in Prototyper; Runtime only
//! reports the trap classes it observes.

pub use csr::values::trap::Exception;
use spin::Once;

static COUNTERS: Once<Option<&'static dyn TrapCounters>> = Once::new();

/// Firmware-event sink observed by Runtime's trap mechanism.
pub trait TrapCounters: Sync {
    /// Reports one observed exception before emulation or forwarding.
    fn record(&self, exception: Exception);
}

/// Publishes the optional firmware-event sink once during boot.
pub fn install(counters: Option<&'static dyn TrapCounters>) {
    COUNTERS.call_once(|| counters);
}

/// Returns the published firmware-event sink.
pub(crate) fn get() -> Option<&'static dyn TrapCounters> {
    COUNTERS.get().and_then(|counters| *counters)
}
