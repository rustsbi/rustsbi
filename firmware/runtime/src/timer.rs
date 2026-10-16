//! Runtime's platform timer service.
//!
//! Prototyper supplies the platform timer through [`TimerDevice`].
//! Runtime clears machine timer sources and reads time through this module.
//! A direct device word takes precedence over the CSR and full device counter.

use alloc::boxed::Box;
use csr::{supervisor, user};
use spin::Once;

static TIMER: Once<Option<&'static dyn TimerDevice>> = Once::new();
// CSR availability belongs to each hart; mixed-hart platforms need separate results.
static SSTC: Once<Box<[Once<bool>]>> = Once::new();

/// One architectural time-register word.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TimeWord {
    /// The `time` register, including all 64 bits on RV64.
    Low,
    /// The upper 32 bits from RV32's `timeh` register.
    #[cfg(target_pointer_width = "32")]
    High,
}

/// Reads one architectural time word from the selected time source.
// Callers are internal trap handlers running in the installed Runtime context.
pub(crate) fn read_time(word: TimeWord) -> Option<usize> {
    let timer = get();
    let direct = timer.and_then(|timer| match word {
        TimeWord::Low => timer.read_time_low(),
        #[cfg(target_pointer_width = "32")]
        TimeWord::High => timer.read_time_high(),
    });
    if direct.is_some() {
        return direct;
    }
    // SAFETY: Internal callers run in Runtime after trap entry is installed.
    // Time reads have no side effects; recovery handles an unavailable CSR.
    let hardware = unsafe {
        match word {
            TimeWord::Low => crate::csr::read::<user::Time>(),
            #[cfg(target_pointer_width = "32")]
            TimeWord::High => crate::csr::read::<user::TimeHigh>(),
        }
    };
    hardware.ok().map(usize::from).or_else(|| {
        let time = timer?.read_time()?;
        Some(match word {
            TimeWord::Low => time as usize,
            #[cfg(target_pointer_width = "32")]
            TimeWord::High => (time >> 32) as usize,
        })
    })
}

/// Returns whether the current hart permits Sstc timer-compare reads.
///
/// The result is cached for this hart after Runtime startup installs trap entry.
pub fn has_sstc() -> bool {
    let capabilities = SSTC.call_once(|| crate::hart::HartId::all().map(|_| Once::new()).collect());
    *capabilities[crate::hart::current_hart().index()].call_once(|| {
        // SAFETY: Runtime startup installed trap entry. Timer-compare reads
        // have no side effects and execute in this hart's M-mode context.
        unsafe { crate::csr::read::<supervisor::TimerCompare>() }.is_ok()
    })
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
