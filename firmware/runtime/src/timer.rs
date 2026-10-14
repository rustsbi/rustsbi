//! Runtime's platform timer service.
//!
//! Prototyper supplies the platform timer through [`TimerDevice`].
//! Runtime clears machine timer sources and reads time through this module.
//! A direct device word takes precedence over the CSR and full device counter.

use alloc::boxed::Box;
use spin::Once;

static TIMER: Once<Option<&'static dyn TimerDevice>> = Once::new();
// CSR availability belongs to each hart; mixed-hart platforms need separate results.
static SSTC: Once<Box<[Once<bool>]>> = Once::new();

/// Returns whether the current hart permits Sstc timer-compare reads.
///
/// The result is cached for this hart after Runtime startup installs trap entry.
pub fn has_sstc() -> bool {
    let capabilities = SSTC.call_once(|| crate::hart::HartId::all().map(|_| Once::new()).collect());
    *capabilities[crate::hart::current_hart().index()].call_once(crate::csr::has_stimecmp)
}

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

    /// Acknowledges timer expiry after `MTIE` is masked.
    ///
    /// The default cancels the timer through [`Self::clear_current`].
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
