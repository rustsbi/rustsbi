//! Timer device ownership, hart-local bindings, and supervisor comparison registers.

mod device;
mod event;
mod machine;
mod schedule;
mod time;

use crate::{
    Error, Result,
    csr::csrr,
    hart::{HartId, PerHart},
    irq::{self, LocalIrqDisabled},
};
use alloc::boxed::Box;
use csr::{machine as registers, supervisor};
use machine::MachineTimer;
use schedule::{Schedule, TimerTarget};
use spin::{Mutex, Once};
use time::CounterSource;

pub use device::TimerDevice;
pub use event::TimerEvent;

static TIMER: Once<Timer> = Once::new();

/// Creates and installs the supervisor timer service.
///
/// Returns [`Error::NotReady`] before Runtime publishes the hart topology.
/// Returns [`Error::AlreadyInitialized`] if a timer is already installed.
/// The supplied device is consumed even when initialization fails.
pub fn init(device: Option<Box<dyn TimerDevice>>) -> Result<&'static Timer> {
    let mut installed = false;
    let timer = TIMER.try_call_once(|| -> Result<Timer> {
        let timer = Timer::new(device)?;
        installed = true;
        Ok(timer)
    })?;
    if installed {
        Ok(timer)
    } else {
        Err(Error::AlreadyInitialized)
    }
}

/// Returns the installed supervisor timer service.
pub fn get() -> Option<&'static Timer> {
    TIMER.get()
}

enum Comparator {
    Native,
    Sstc,
}

struct HartTimer {
    supervisor: Once<Comparator>,
    event: Once<TimerEvent>,
    state: Mutex<MachineState>,
}

#[derive(Default)]
struct MachineState {
    clock: Option<CounterSource>,
    schedule: Schedule,
}

/// A timer service for the current hart.
///
/// Operations select the current hart and exclude local interrupt reentry.
/// Supervisor deadline operations return [`Error::NotReady`] on an uninitialized hart.
/// A firmware event remains registered for the firmware lifetime.
pub struct Timer {
    machine: Option<MachineTimer>,
    local: PerHart<HartTimer>,
}

impl Timer {
    fn new(device: Option<Box<dyn TimerDevice>>) -> Result<Self> {
        Ok(Self {
            machine: device.map(MachineTimer::new),
            local: PerHart::new(|_| HartTimer {
                supervisor: Once::new(),
                event: Once::new(),
                state: Mutex::new(MachineState::default()),
            })?,
        })
    }

    /// Returns whether this hart has an initialized timer binding.
    pub fn is_available(&self) -> bool {
        let hart = HartId::current().expect("hart-local access before topology publication");
        self.local.get(hart).supervisor.get().is_some()
    }

    /// Replaces the current hart's absolute supervisor deadline.
    ///
    /// A deadline of `u64::MAX` cancels the timer.
    pub fn set_deadline(&self, deadline: u64) -> Result<()> {
        irq::with_local_disabled(|irq| {
            let hart = HartId::current()?;
            let compare = self
                .local
                .get(hart)
                .supervisor
                .get()
                .ok_or(Error::NotReady)?;
            match compare {
                Comparator::Native => {
                    self.set_native_supervisor_deadline(hart, Some(deadline), irq)
                }
                Comparator::Sstc => set_supervisor_deadline(deadline, irq),
            }
            Ok(())
        })
    }

    /// Cancels the current hart's supervisor deadline.
    ///
    /// A pending supervisor timer interrupt may take time to clear when Sstc is used.
    pub fn cancel(&self) -> Result<()> {
        irq::with_local_disabled(|irq| self.cancel_with(irq))
    }

    fn cancel_with(&self, irq: &LocalIrqDisabled) -> Result<()> {
        let hart = HartId::current()?;
        let compare = self
            .local
            .get(hart)
            .supervisor
            .get()
            .ok_or(Error::NotReady)?;
        match compare {
            Comparator::Native => self.set_native_supervisor_deadline(hart, None, irq),
            Comparator::Sstc => set_supervisor_deadline(u64::MAX, irq),
        }
        Ok(())
    }

    pub(crate) fn init_current(&self, irq: &LocalIrqDisabled) -> Result<()> {
        if !crate::boot::traps_ready() {
            return Err(Error::NotReady);
        }
        let hart = HartId::current()?;
        let local = self.local.get(hart);
        if local.supervisor.get().is_some() {
            return Ok(());
        }
        if Self::sstc_available()? {
            local.supervisor.call_once(|| Comparator::Sstc);
            return self.cancel_with(irq);
        }
        if self.machine().is_none() {
            // Platforms without native hardware or Sstc do not expose TIME.
            return Ok(());
        }
        let clock = self.counter_source();
        local.state.lock().clock = clock;
        local.supervisor.call_once(|| Comparator::Native);
        self.cancel_with(irq)
    }

    fn set_native_supervisor_deadline(
        &self,
        hart: HartId,
        deadline: Option<u64>,
        irq: &LocalIrqDisabled,
    ) {
        // SAFETY: The native supervisor timer owns STIP on the current hart.
        unsafe { registers::InterruptPending::clear_bit::<registers::STIP>() };
        self.update_deadline(hart, TimerTarget::Supervisor, deadline, irq);
    }

    fn update_deadline(
        &self,
        hart: HartId,
        target: TimerTarget,
        deadline: Option<u64>,
        irq: &LocalIrqDisabled,
    ) {
        let machine = self.machine().expect("native timer unavailable");
        let mut state = self.local.get(hart).state.lock();
        let next = state.schedule.set(target, deadline);
        machine.set_next_deadline(hart, next, irq);
    }

    pub(crate) fn cancel_events_on_stop(&self, hart: HartId, irq: &LocalIrqDisabled) {
        let local = self.local.get(hart);
        match local.supervisor.get() {
            Some(Comparator::Native) => self.set_native_supervisor_deadline(hart, None, irq),
            Some(Comparator::Sstc) => set_supervisor_deadline(u64::MAX, irq),
            None => {}
        }
        if local.event.get().is_some() {
            self.update_deadline(hart, TimerTarget::Firmware, None, irq);
        }
    }

    /// Returns whether the supervisor timer compare register is accessible on this hart.
    ///
    /// Returns `false` before Runtime installs the trap handler.
    /// This does not report whether supervisor access is enabled.
    pub fn supports_sstc() -> bool {
        crate::boot::traps_ready() && csrr!(supervisor::TimerCompare).is_ok()
    }

    fn sstc_available() -> Result<bool> {
        #[cfg(target_pointer_width = "64")]
        type Configuration = registers::EnvironmentConfig;
        #[cfg(target_pointer_width = "32")]
        type Configuration = registers::EnvironmentConfigHigh;
        #[cfg(target_pointer_width = "64")]
        type TimerEnable = registers::STCE;
        #[cfg(target_pointer_width = "32")]
        type TimerEnable = registers::STCEH;
        let enabled = csrr!(Configuration).is_ok_and(|config| {
            #[cfg(target_pointer_width = "64")]
            let config = registers::EnvironmentConfig::from_bits(config);
            #[cfg(target_pointer_width = "32")]
            let config = registers::EnvironmentConfigHigh::from_bits(config);
            config.get::<TimerEnable>() == registers::FeatureState::Enabled
        });
        if !enabled {
            return Ok(false);
        }
        csrr!(supervisor::TimerCompare).map_err(|_| Error::NotSupported)?;
        #[cfg(target_pointer_width = "32")]
        csrr!(supervisor::TimerCompareHigh).map_err(|_| Error::NotSupported)?;
        Ok(true)
    }

    pub(crate) fn on_interrupt(&self, irq: &LocalIrqDisabled) {
        let hart = HartId::current().expect("MTIP on an unknown hart");
        let local = self.local.get(hart);
        let supervisor = local
            .supervisor
            .get()
            .expect("MTIP without an initialized timer binding");
        let firmware = local.event.get();
        if matches!(supervisor, Comparator::Sstc) && firmware.is_none() {
            panic!("MTIP without a firmware timer event");
        }
        let machine = self.machine().expect("MTIP without native timer hardware");
        machine.acknowledge_interrupt(hart, irq);
        let clock = local.state.lock().clock;
        let now = if firmware.is_some() {
            clock
                .and_then(|clock| self.read_counter(clock))
                .expect("firmware timer counter became unavailable")
        } else if matches!(supervisor, Comparator::Native) {
            // Preserve supervisor timer support on devices without a readable
            // counter. Such devices cannot filter residual interrupts.
            clock
                .and_then(|clock| self.read_counter(clock))
                .unwrap_or(u64::MAX)
        } else {
            unreachable!("MTIP without a native timer event");
        };
        let expired = {
            let mut state = local.state.lock();
            let expired = state.schedule.take_expired(now);
            machine.set_next_deadline(hart, state.schedule.next(), irq);
            expired
        };
        if expired.supervisor {
            assert!(
                matches!(supervisor, Comparator::Native),
                "supervisor deadline without a native timer binding"
            );
            // SAFETY: The native supervisor timer owns STIP on the current hart.
            unsafe { registers::InterruptPending::set_bit::<registers::STIP>() };
        }
        if expired.firmware {
            firmware.expect("firmware timer event missing").deliver(irq);
        }
    }

    fn machine(&self) -> Option<&MachineTimer> {
        self.machine.as_ref()
    }
}

fn set_supervisor_deadline(deadline: u64, _irq: &LocalIrqDisabled) {
    // SAFETY:
    // 1. Per-hart initialization confirmed Sstc and STCE.
    // 2. The caller excludes interrupt reentry throughout the comparison update.
    unsafe {
        #[cfg(target_pointer_width = "64")]
        supervisor::TimerCompare::write((deadline as usize).into());
        #[cfg(target_pointer_width = "32")]
        {
            supervisor::TimerCompare::write(usize::MAX.into());
            supervisor::TimerCompareHigh::write(((deadline >> 32) as usize).into());
            supervisor::TimerCompare::write((deadline as usize).into());
        }
    }
}
