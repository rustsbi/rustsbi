//! Runtime's platform timer service.
//!
//! Prototyper constructs the concrete timer implementation. Runtime uses
//! this service to clear a machine timer source and, when available, read a
//! device counter after the guarded architectural `time` CSR access fails.

use spin::Once;

static TIMER: Once<Option<&'static dyn TimerDevice>> = Once::new();

/// The platform timer device used by the machine timer transport and the
/// `time` CSR emulation fallback.
pub trait TimerDevice: Sync {
    /// Clears the current hart's platform timer source.
    fn clear_current(&self);

    /// Reads a device-provided time counter when one exists.
    fn read_time(&self) -> Option<u64>;
}

/// Publishes the platform timer service once during boot.
pub fn install(device: Option<&'static dyn TimerDevice>) {
    TIMER.call_once(|| device);
}

/// Returns the published platform timer service.
pub(crate) fn get() -> Option<&'static dyn TimerDevice> {
    TIMER.get().and_then(|device| *device)
}
