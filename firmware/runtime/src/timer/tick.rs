//! Platform time counter reads.
//!
//! [`Tick`] is detected once per platform and published by firmware. Firmware
//! and the `rdtime` emulation path both read it through [`now`].

use super::TimerDevice;
use crate::{Error, Result, boot, hart::Detect};
use csr::{machine as registers, user};
use spin::Once;

/// Platform time counter.
///
/// Detection decides once per platform between the architectural `time` CSR
/// and the emulation path.
#[derive(Clone, Copy)]
enum Tick {
    /// The hart implements the `time` CSR; reads are direct.
    Csr,
    /// The hart has no `time` CSR: `rdtime` traps and emulation synthesizes
    /// the value from this device.
    Emulated(&'static dyn TimerDevice),
    /// The hart has no `time` CSR and the platform has no counter device, so
    /// `rdtime` cannot be emulated.
    Unavailable,
}

impl Tick {
    /// Detects how the current hart reads the time counter.
    fn detect(device: Option<&'static dyn TimerDevice>) -> Result<Self> {
        if !boot::traps_ready() || usize::from(registers::Scratch::read()) != 0 {
            return Err(Error::NotReady);
        }
        // SAFETY: Runtime trap entry is installed for this hart and `mscratch`
        // is zero, so the probe recovers an absent CSR.
        let csr = unsafe { user::Time::detect() }.is_ok();
        #[cfg(target_pointer_width = "32")]
        let csr = csr && unsafe { user::TimeHigh::detect() }.is_ok();
        Ok(match (csr, device) {
            (true, _) => Self::Csr,
            (false, Some(device)) => Self::Emulated(device),
            (false, None) => Self::Unavailable,
        })
    }

    fn read(&self) -> Option<u64> {
        match self {
            Self::Csr => Some(user::Time::read64()),
            Self::Emulated(device) => device.read_time(),
            Self::Unavailable => None,
        }
    }
}

static TICK: Once<Tick> = Once::new();

/// Publishes the platform time source read by [`now`].
///
/// Firmware calls this once after publishing its policy.
pub fn init(device: Option<&'static dyn TimerDevice>) -> Result<()> {
    let tick = Tick::detect(device)?;
    TICK.call_once(|| tick);
    Ok(())
}

/// Reads the current platform time counter.
///
/// Returns `None` when detection found no source at all.
pub fn now() -> Option<u64> {
    TICK.get().and_then(|tick| tick.read())
}
