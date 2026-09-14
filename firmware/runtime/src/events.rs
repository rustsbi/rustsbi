//! Runtime's firmware-event accounting service.
//!
//! This is separate from the hardware IPI and timer services. The concrete
//! PMU accounting policy remains in Prototyper; Runtime only reports the
//! trap classes it observes.

use spin::Once;

static COUNTERS: Once<Option<&'static dyn TrapCounters>> = Once::new();

/// Firmware-event sink observed by Runtime's trap mechanism.
pub trait TrapCounters: Sync {
    /// Records an illegal-instruction trap.
    fn record_illegal_instruction(&self);

    /// Records a misaligned-load trap.
    fn record_misaligned_load(&self);

    /// Records a misaligned-store trap.
    fn record_misaligned_store(&self);

    /// Records a load access fault.
    fn record_access_load(&self);

    /// Records a store access fault.
    fn record_access_store(&self);
}

/// Publishes the optional firmware-event sink once during boot.
pub fn install(counters: Option<&'static dyn TrapCounters>) {
    COUNTERS.call_once(|| counters);
}

/// Returns the published firmware-event sink.
pub(crate) fn get() -> Option<&'static dyn TrapCounters> {
    COUNTERS.get().and_then(|counters| *counters)
}
