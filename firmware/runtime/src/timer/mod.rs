//! Supervisor timer comparison registers, platform machine timers, and time reads.

mod tick;

use crate::{
    Error, Result, boot,
    hart::{Detect, HartId, PerHart},
    irq::{self, LocalIrqDisabled},
};
use alloc::boxed::Box;
use csr::{machine as registers, supervisor};
use spin::Once;

/// Supervisor comparison register access.
///
/// The value exists only after this hart's comparison registers were probed.
struct Sstc(());

impl Sstc {
    fn detect() -> Result<Option<Self>> {
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

    /// Replaces the current hart's absolute supervisor deadline.
    ///
    /// `u64::MAX` cancels the deadline.
    /// A pending supervisor timer interrupt may take time to clear.
    fn set_deadline(&self, deadline: u64) {
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

/// The platform timer and current-hart supervisor timer delivery.
///
/// The boot hart initializes the platform service once.
/// Each hart initializes its own supervisor comparison registers with
/// [`init_hart`] before use.
pub struct Timer {
    device: Option<Box<dyn TimerDevice>>,
    sstc: PerHart<Once<Option<Sstc>>>,
}

impl Timer {
    fn current_sstc(&self) -> Option<&Sstc> {
        let hart = HartId::current().expect("BUG: current hart is not in the boot topology");
        self.sstc
            .get(hart)
            .get()
            .expect("BUG: hart timer used before initialization")
            .as_ref()
    }

    /// Sets the current hart's supervisor deadline, if a timer is available.
    ///
    /// `u64::MAX` cancels the deadline.
    /// The current hart must have completed [`init_hart`].
    ///
    /// # Panics
    ///
    /// Panics if the current hart has not completed [`init_hart`] or is absent
    /// from the published topology.
    pub fn set_deadline(&self, deadline: u64) {
        match (self.current_sstc(), &self.device) {
            (Some(sstc), _) => sstc.set_deadline(deadline),
            (None, Some(device)) => device.set_deadline(deadline),
            (None, None) => {}
        }
    }
}

static TIMER: Once<Timer> = Once::new();

/// Registers the platform timer and detects the platform time source.
///
/// A second registration returns [`Error::AlreadyInitialized`].
/// The boot hart must publish the hart topology and install trap recovery
/// before this call.
pub fn init(device: Option<Box<dyn TimerDevice>>) -> Result<()> {
    let mut initialized = false;
    TIMER.try_call_once(|| {
        let timer = Timer {
            device,
            sstc: PerHart::new(|_| Once::new())?,
        };
        tick::init(tick::Tick::detect()?);
        initialized = true;
        Ok(timer)
    })?;
    initialized.then_some(()).ok_or(Error::AlreadyInitialized)
}

/// Detects and records supervisor timer support for the current hart.
///
/// Returns whether the hart supports Sstc.
/// Runtime trap recovery and the platform timer must be initialized before
/// this call.
pub fn init_hart() -> Result<bool> {
    let timer = TIMER.get().ok_or(Error::NotReady)?;
    let hart = HartId::current()?;
    timer
        .sstc
        .get(hart)
        .try_call_once(Sstc::detect)
        .map(|sstc| sstc.is_some())
}

/// Returns the initialized timer available to the current hart, if present.
///
/// When the timer service exists without a platform timer device, the current
/// hart must have completed [`init_hart`].
///
/// # Panics
///
/// Panics if the timer service is registered without a platform timer device
/// and the current hart is absent from the topology or has not completed
/// [`init_hart`].
pub fn get() -> Option<&'static Timer> {
    let timer = TIMER.get()?;
    (timer.device.is_some() || timer.current_sstc().is_some()).then_some(timer)
}

/// Reads the current platform time counter, if a source is available.
pub fn now() -> Option<u64> {
    tick::now()
}

/// A platform machine timer block.
///
/// Drivers own validated MMIO and the compare-safe write rules.
/// Runtime owns the interrupt state that must accompany a comparator update,
/// so drivers implement [`TimerDevice::set_compare`] and inherit
/// [`TimerDevice::set_deadline`].
pub trait TimerDevice: Send + Sync {
    /// Programs one hart's comparator.
    ///
    /// `u64::MAX` cancels the deadline.
    /// Split writes must not expose a transient deadline that has already expired.
    fn set_compare(&self, hart: HartId, deadline: u64);

    /// Reads this block's counter when it exposes one.
    ///
    /// Timer delivery never reads the counter; platform workarounds use this
    /// for bounded waits.
    /// The counter and the comparator must share one time base.
    fn read_time(&self) -> Option<u64> {
        None
    }

    /// Programs the current hart's supervisor deadline.
    ///
    /// `u64::MAX` cancels the deadline.
    /// Clears the pending supervisor timer interrupt.
    /// Enables the machine timer interrupt for deadlines other than `u64::MAX`.
    fn set_deadline(&self, deadline: u64) {
        let hart = HartId::current().expect("machine timer on a hart outside the topology");
        // Keep the interrupt-state changes and comparator update together so
        // delivery cannot be rearmed with the previous deadline.
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
/// This changes only `mip.STIP`.
/// With Sstc, `STIP` is driven by `stimecmp` and is read-only,
/// so firmware calls this only for native machine-timer delivery.
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
