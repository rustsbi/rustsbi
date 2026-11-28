//! Supervisor timer comparison registers, platform machine timers, and time reads.

mod sstc;
mod tick;

use csr::{
    Clear, Set, Write,
    standard::isa::privileged::machine::{Mie, Mip},
};

use crate::{
    Error, Result,
    hart::{HartId, PerHart},
    irq::{self, LocalIrqDisabled},
};
use alloc::boxed::Box;
use spin::Once;
use sstc::Sstc;
use tick::Tick;

/// The platform timer and current-hart supervisor timer delivery.
///
/// The boot hart initializes the platform service once.
/// Each hart initializes its own supervisor comparison registers with
/// [`init_hart`] before use.
pub struct Timer {
    device: Option<Box<dyn TimerDevice>>,
    harts: PerHart<HartTimer>,
}

#[derive(Default)]
struct HartTimer {
    tick: Once<Tick>,
    sstc: Once<Sstc>,
}

impl Timer {
    fn sstc(&self, hart: HartId) -> Option<&Sstc> {
        self.harts.get(hart).sstc.get()
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
    /// timer device are also returned. If programming the device fails, Runtime
    /// clears the supervisor pending bit and leaves the machine timer interrupt
    /// masked. The device may already have changed part of its comparator.
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

/// Registers the platform timer and detects the boot hart's time source.
///
/// A second registration returns [`Error::AlreadyInitialized`].
/// The boot hart must publish the hart topology and install trap recovery
/// before this call.
pub fn init(device: Option<Box<dyn TimerDevice>>) -> Result<()> {
    let mut initialized = false;
    TIMER.try_call_once(|| -> Result<Timer> {
        let timer = Timer {
            device,
            harts: PerHart::new(|_| HartTimer::default())?,
        };
        Tick::get(&timer.harts.get(HartId::current()?).tick)?;
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
    let state = timer.harts.get(hart);
    Tick::get(&state.tick)?;
    let slot = &state.sstc;
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

/// Reads this hart's `time` CSR or the platform timer's counter, if available.
///
/// Returns `Ok(None)` when neither source exposes a counter. Returns
/// [`Error::NotReady`] before registration or if this hart still needs to
/// probe its CSR and trap recovery is unavailable. Device errors propagate.
pub fn now() -> Result<Option<u64>> {
    let timer = TIMER.get().ok_or(Error::NotReady)?;
    let state = timer.harts.get(HartId::current()?);
    Tick::get(&state.tick)?.read(timer.device.as_deref())
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
    /// Return `Ok(None)` for a block without a readable counter. A counter
    /// value must use the same time base as [`Self::set_compare`].
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
    // SAFETY: `_irq` proves local interrupts are disabled, and only MTIE is
    // edited in the current hart's interrupt-enable register.
    unsafe {
        if enabled {
            Mie::write(Set(Mie::MTIE));
        } else {
            Mie::write(Clear(Mie::MTIE));
        }
    }
}

/// Posts or clears a supervisor timer interrupt during machine-timer delivery.
///
/// With `menvcfg.STCE=1`, hardware derives `mip.STIP` from `stimecmp`.
fn set_supervisor_pending(_irq: &LocalIrqDisabled, pending: bool) {
    // SAFETY: `_irq` proves local interrupts are disabled, the current hart
    // owns supervisor timer delivery, and only the fixed STIP field is updated.
    unsafe {
        if pending {
            Mip::write(Set(Mip::STIP));
        } else {
            Mip::write(Clear(Mip::STIP));
        }
    }
}
