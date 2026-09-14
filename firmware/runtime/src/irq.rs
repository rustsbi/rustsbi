//! Runtime's external-interrupt controller service.
//!
//! An AIA/IMSIC platform may deliver the firmware IPI through a machine
//! external interrupt instead of the CLINT software-interrupt source. The
//! claim operation acknowledges the selected IMSIC IPI identity. This is the
//! only external-interrupt operation currently required by Runtime.

use spin::Once;

static CONTROLLER: Once<Option<&'static dyn ExternalInterrupt>> = Once::new();

/// External-interrupt operation needed by Runtime's machine transport.
pub trait ExternalInterrupt: Sync {
    /// Claims the current hart's external interrupt when it is the firmware
    /// IPI identity. The claim acknowledges the controller source.
    fn claim_ipi(&self) -> bool;
}

/// Publishes the optional external-interrupt controller once during boot.
pub fn install(controller: Option<&'static dyn ExternalInterrupt>) {
    CONTROLLER.call_once(|| controller);
}

/// Returns the published external-interrupt controller.
pub(crate) fn get() -> Option<&'static dyn ExternalInterrupt> {
    CONTROLLER.get().and_then(|controller| *controller)
}
