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
    /// Reads a device-provided time counter when one exists.
    fn read_time(&self) -> Option<u64>;

    /// Reads a direct device counter word without probing the architecture CSR.
    #[inline]
    fn read_time_low(&self) -> Option<usize> {
        None
    }

    /// Reads a direct device counter high word on RV32.
    #[cfg(target_pointer_width = "32")]
    #[inline]
    fn read_time_high(&self) -> Option<usize> {
        None
    }

    /// Clears the current hart's platform timer source.
    fn clear_current(&self);

    /// Acknowledges expiry after MTIE is masked, retaining cancellation as the default.
    fn acknowledge_current(&self) {
        self.clear_current();
    }
}

/// Publishes the platform timer service once during boot.
pub fn install(device: Option<&'static dyn TimerDevice>) {
    TIMER.call_once(|| device);
}

/// Returns the published platform timer service.
pub(crate) fn get() -> Option<&'static dyn TimerDevice> {
    TIMER.get().and_then(|device| *device)
}
