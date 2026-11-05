//! Selection and reading of the platform time counter.

use crate::{Error, Result, boot, hart::Detect, irq};
use csr::{machine as registers, user};
use spin::Once;

/// The counter source used by the platform timer.
pub(super) enum Tick {
    /// The architectural `time` CSR.
    Csr,
    /// A counter read through the registered machine timer device.
    Device,
}

impl Tick {
    pub(super) fn detect() -> Result<Self> {
        irq::with_local_disabled(|_| {
            if !boot::traps_ready() || usize::from(registers::Scratch::read()) != 0 {
                return Err(Error::NotReady);
            }
            // SAFETY: Runtime trap entry is installed, local interrupts are
            // excluded, and `mscratch` is zero, so an absent `time` CSR is
            // recovered.
            let present = unsafe { user::Time::detect() }.is_ok();
            #[cfg(target_pointer_width = "32")]
            // SAFETY: The same probe conditions apply: Runtime trap entry is
            // installed, local interrupts are excluded, and `mscratch` is zero,
            // so an absent `timeh` CSR is recovered.
            let present = present && unsafe { user::TimeHigh::detect() }.is_ok();
            Ok(if present { Self::Csr } else { Self::Device })
        })
    }

    fn read(&self) -> Result<Option<u64>> {
        match self {
            Self::Csr => Ok(Some(user::Time::read64())),
            Self::Device => match &super::TIMER.get().ok_or(Error::NotReady)?.device {
                Some(device) => device.read_time(),
                None => Ok(None),
            },
        }
    }
}

static TICK: Once<Tick> = Once::new();

/// Publishes the source selected during timer initialization.
pub(super) fn init(tick: Tick) {
    TICK.call_once(|| tick);
}

/// Reads the selected counter if it is available.
pub(super) fn now() -> Result<Option<u64>> {
    TICK.get().ok_or(Error::NotReady)?.read()
}
