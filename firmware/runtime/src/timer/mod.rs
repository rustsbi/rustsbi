//! Supervisor timer comparison registers, platform machine timers, and time reads.

mod tick;

pub use tick::{init, now};

use crate::{
    Error, Result, boot,
    hart::{Detect, HartId},
    irq::{self, LocalIrqDisabled},
};
use csr::{machine as registers, supervisor};

/// Supervisor comparison register access.
///
/// [`Timer`] exists only after [`Timer::detect`] found both `stimecmp` words on
/// this platform, so the type itself carries the availability decision. The
/// private field keeps other crates from constructing one without a probe.
pub struct Timer(());

impl Timer {
    /// Probes the current hart for the supervisor comparison registers.
    ///
    /// Returns `Ok(None)` when the platform has no Sstc comparison registers.
    /// Returns [`Error::NotReady`] before Runtime installs trap recovery, or
    /// when called from a machine trap frame.
    pub fn detect() -> Result<Option<Self>> {
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
    }

    /// Replaces the current hart's absolute supervisor deadline.
    ///
    /// `u64::MAX` cancels the deadline. A pending supervisor timer interrupt
    /// may take time to clear.
    pub fn set_deadline(&self, deadline: u64) {
        irq::with_local_disabled(|_| {
            // SAFETY:
            // 1. Detection established both comparison words on this hart.
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

/// A platform machine timer block.
///
/// Drivers own validated MMIO and the compare-safe write rules. Runtime owns
/// the interrupt state that must accompany a comparator update, so drivers
/// implement [`TimerDevice::set_compare`] and inherit [`TimerDevice::set_deadline`].
pub trait TimerDevice: Send + Sync {
    /// Programs one hart's comparator.
    ///
    /// `u64::MAX` cancels the deadline. Split writes must not expose a
    /// transient deadline that has already expired.
    fn set_compare(&self, hart: HartId, deadline: u64);

    /// Reads this block's counter when it exposes one.
    ///
    /// Timer delivery never reads the counter; platform workarounds use this
    /// for bounded waits. The counter and the comparator must share one time
    /// base.
    fn read_time(&self) -> Option<u64> {
        None
    }

    /// Clears the pending supervisor timer interrupt, programs `deadline`, and
    /// sets the machine timer interrupt enable to match.
    ///
    /// The three steps stay together so no caller can rearm delivery while the
    /// comparator still holds the previous deadline.
    fn set_deadline(&self, deadline: u64) {
        let hart = HartId::current().expect("machine timer on a hart outside the topology");
        irq::with_local_disabled(|irq| {
            set_supervisor_pending(irq, false);
            self.set_compare(hart, deadline);
            set_machine_timer_enabled(irq, deadline != u64::MAX);
        });
    }
}

/// Masks the machine timer source and reports the interrupt to the supervisor.
///
/// `MTIP` stays asserted until the next [`TimerDevice::set_deadline`] moves the
/// comparator, so the source is masked here to keep it from re-firing.
pub(crate) fn on_interrupt(irq: &LocalIrqDisabled) {
    set_machine_timer_enabled(irq, false);
    set_supervisor_pending(irq, true);
}

/// Enables or disables the current hart's machine timer interrupt source.
///
/// This changes only `MTIE` and preserves the other enable bits.
fn set_machine_timer_enabled(_irq: &LocalIrqDisabled, enabled: bool) {
    let state = if enabled {
        registers::Interrupt::Enabled
    } else {
        registers::Interrupt::Disabled
    };
    // SAFETY: `_irq` proves local interrupts are disabled, and only MTIE is
    // edited in the current hart's interrupt-enable register.
    unsafe {
        registers::InterruptEnable::modify(|_, value| value.set::<registers::MTIE>(state));
    }
}

/// Sets or clears the current hart's supervisor timer pending bit.
///
/// This changes only `mip.STIP`. With Sstc, `STIP` is driven by `stimecmp` and
/// is read-only, so firmware calls this only for native machine-timer delivery.
fn set_supervisor_pending(_irq: &LocalIrqDisabled, pending: bool) {
    // SAFETY: `_irq` proves local interrupts are disabled, the current hart
    // owns supervisor timer delivery, and only the fixed STIP field is updated.
    unsafe {
        if pending {
            registers::InterruptPending::set_bit::<registers::STIP>();
        } else {
            registers::InterruptPending::clear_bit::<registers::STIP>();
        }
    }
}
