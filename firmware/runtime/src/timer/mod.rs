//! Supervisor timer comparison registers, platform machine timers, and time reads.

mod sstc;
mod tick;

use crate::{
    Error, Result,
    hart::{HartId, PerHart},
    irq::{self, LocalIrqDisabled},
};
use alloc::boxed::Box;
use csr::machine as registers;
use spin::Once;
use sstc::Sstc;

/// The platform timer and current-hart supervisor timer delivery.
///
/// The boot hart initializes the platform service once.
/// Each hart initializes its own supervisor comparison registers with
/// [`init_hart`] before use.
pub struct Timer {
    device: Option<Box<dyn TimerDevice>>,
    sstc: PerHart<Once<Sstc>>,
}

impl Timer {
    fn sstc(&self, hart: HartId) -> Option<&Sstc> {
        self.sstc.get(hart).get()
    }

    /// Sets the current hart's supervisor deadline.
    ///
    /// `u64::MAX` cancels the deadline.
    /// The current hart must have completed [`init_hart`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::NotReady`] if the hart topology is not published,
    /// [`Error::InvalidArgs`] if the current hart is absent from it, or
    /// [`Error::NotSupported`] if neither timer is available. Errors from the
    /// timer device are also returned.
    pub fn set_deadline(&self, deadline: u64) -> Result<()> {
        let current_hart = HartId::current()?;
        match (self.sstc(current_hart), &self.device) {
            (Some(sstc), _) => {
                sstc.set_deadline(deadline);
                Ok(())
            }
            (None, Some(device)) => irq::with_local_disabled(|irq| {
                // Keep the comparator and interrupt-state updates together.
                set_supervisor_pending(irq, false);
                if let Err(error) = device.set_compare(current_hart, deadline) {
                    set_machine_timer_enabled(irq, false);
                    return Err(error);
                }
                set_machine_timer_enabled(irq, deadline != u64::MAX);
                Ok(())
            }),
            (None, None) => Err(Error::NotSupported),
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

/// Initializes supervisor timer support for the current hart.
///
/// Runtime trap recovery and the platform timer must be initialized before
/// this call.
/// A detected Sstc is retained per hart. Supervisor access is configured on
/// each call so it can be restored after a hardware reset.
/// A hart without Sstc is probed again on a later call.
pub fn init_hart() -> Result<()> {
    let timer = TIMER.get().ok_or(Error::NotReady)?;
    let hart = HartId::current()?;
    let slot = timer.sstc.get(hart);
    let sstc = match slot.get() {
        Some(sstc) => Some(sstc),
        None => Sstc::detect()?.map(|sstc| slot.call_once(|| sstc)),
    };
    if let Some(sstc) = sstc {
        sstc.enable_supervisor_access();
    }
    Ok(())
}

/// Returns the initialized timer available to the current hart, if present.
///
/// The current hart must have completed [`init_hart`].
///
/// Returns [`Error::NotReady`] before the service is initialized, and
/// [`Error::InvalidArgs`] if the hart is absent from the topology.
pub fn get() -> Result<Option<&'static Timer>> {
    let timer = TIMER.get().ok_or(Error::NotReady)?;
    let current_hart = HartId::current()?;
    let sstc = timer.sstc(current_hart);
    Ok((timer.device.is_some() || sstc.is_some()).then_some(timer))
}

/// Reads the current platform time counter, if a source is available.
///
/// Returns [`Error::NotReady`] before the timer service is initialized, and
/// propagates errors from the selected counter device.
pub fn now() -> Result<Option<u64>> {
    tick::now()
}

/// A platform machine timer block.
///
/// Drivers own validated MMIO and the compare-safe write rules. Runtime owns
/// the interrupt state that must accompany a comparator update.
pub trait TimerDevice: Send + Sync {
    /// Programs one hart's comparator.
    ///
    /// `u64::MAX` cancels the deadline.
    /// Split writes must not expose a transient deadline that has already expired.
    fn set_compare(&self, hart: HartId, deadline: u64) -> Result<()>;

    /// Reads this block's counter when it exposes one.
    ///
    /// Timer delivery never reads the counter; platform workarounds use this
    /// for bounded waits.
    /// The counter and the comparator must share one time base.
    fn read_time(&self) -> Result<Option<u64>> {
        Ok(None)
    }
}

/// Masks the machine timer source and reports the interrupt to the supervisor.
///
/// `MTIP` stays asserted until the next [`Timer::set_deadline`] moves the
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
