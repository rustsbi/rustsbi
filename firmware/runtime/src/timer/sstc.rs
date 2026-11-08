//! Current-hart supervisor timer comparison registers.

use crate::{Error, Result, boot, hart::Detect, irq};
use csr::{machine as registers, supervisor};

/// Supervisor comparison registers detected on the current hart.
pub(super) struct Sstc(());

impl Sstc {
    pub(super) fn detect() -> Result<Option<Self>> {
        irq::with_local_disabled(|_| {
            if !boot::traps_ready() || usize::from(registers::Scratch::read()) != 0 {
                return Err(Error::NotReady);
            }
            // SAFETY: Runtime installed trap recovery, local interrupts are
            // excluded, and this hart is not inside a machine trap frame.
            if unsafe { supervisor::TimerCompare::detect() }.is_err() {
                return Ok(None);
            }
            #[cfg(target_pointer_width = "32")]
            // SAFETY: The same current-hart probe context applies. RV32 also needs
            // the high comparison word to program a 64-bit deadline.
            if unsafe { supervisor::TimerCompareHigh::detect() }.is_err() {
                return Ok(None);
            }
            Ok(Some(Self(())))
        })
    }

    /// Enables supervisor access to the current hart's comparison registers.
    pub(super) fn enable_supervisor_access(&self) {
        #[cfg(target_pointer_width = "64")]
        use csr::machine::{EnvironmentConfig as TimerConfig, STCE as TimerEnable};
        #[cfg(target_pointer_width = "32")]
        use csr::machine::{EnvironmentConfigHigh as TimerConfig, STCEH as TimerEnable};

        irq::with_local_disabled(|_| {
            // SAFETY: Sstc was detected on this hart, and local interrupts are
            // excluded while its supervisor timer access is configured.
            unsafe {
                TimerConfig::modify(|_, value| {
                    value.set::<TimerEnable>(registers::FeatureState::Enabled)
                })
            };
        });
    }

    /// Replaces the current hart's absolute supervisor deadline.
    ///
    /// `u64::MAX` cancels the deadline.
    /// A pending supervisor timer interrupt may take time to clear.
    pub(super) fn set_deadline(&self, deadline: u64) {
        irq::with_local_disabled(|_| {
            // SAFETY:
            // 1. Detection established this target's comparison registers on
            //    this hart (both words on RV32).
            // 2. This scope excludes interrupt reentry across a split write.
            unsafe {
                #[cfg(target_pointer_width = "64")]
                supervisor::TimerCompare::write((deadline as usize).into());
                #[cfg(target_pointer_width = "32")]
                {
                    // Writing the maximum low word first prevents an early
                    // comparison while the high word changes.
                    supervisor::TimerCompare::write(usize::MAX.into());
                    supervisor::TimerCompareHigh::write(((deadline >> 32) as usize).into());
                    supervisor::TimerCompare::write((deadline as usize).into());
                }
            }
        });
    }
}
