//! Runtime's platform IPI service.
//!
//! Prototyper constructs the concrete CLINT or IMSIC implementation. Runtime
//! only depends on these operations while waking harts and transporting
//! machine interrupt work.

use crate::hart::HartId;
use spin::Once;

static DEVICE: Once<Option<&'static dyn IpiDevice>> = Once::new();
static HANDLER: Once<Option<&'static dyn IpiHandler>> = Once::new();

/// The platform IPI device used for hart wakeups and machine interrupt
/// acknowledgement.
pub trait IpiDevice: Sync {
    /// Sends a machine IPI to `hart`.
    fn send(&self, hart: HartId) -> Result<(), ()>;

    /// Clears the current hart's pending software interrupt source.
    fn clear_current(&self) -> Result<(), ()>;
}

/// Handles firmware work recorded for the current hart after the interrupt
/// source has been acknowledged.
pub trait IpiHandler: Sync {
    /// Delivers pending supervisor-software and remote-fence work.
    fn deliver_current(&self);
}

/// Publishes the platform IPI line once during boot.
pub fn install(device: Option<&'static dyn IpiDevice>) {
    DEVICE.call_once(|| device);
}

/// Publishes the firmware-work handler once during boot.
pub fn install_handler(handler: Option<&'static dyn IpiHandler>) {
    HANDLER.call_once(|| handler);
}

/// Returns the published platform IPI service.
pub(crate) fn get() -> Option<&'static dyn IpiDevice> {
    DEVICE.get().and_then(|device| *device)
}

/// Returns the published firmware-work handler.
pub(crate) fn handler() -> Option<&'static dyn IpiHandler> {
    HANDLER.get().and_then(|handler| *handler)
}
