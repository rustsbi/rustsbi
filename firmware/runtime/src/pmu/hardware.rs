//! Physical PMU counter and event-selector instructions.

use super::{Counters, HardwareCounter, HpmCounter, InhibitSupport};
use crate::fault_recovery::{csrr, csrw};
use crate::{Error, Result};
use csr::{
    machine,
    values::counter::{CounterValue, InhibitedCounters},
};
use seq_macro::seq;

pub(super) fn discover() -> Result<[u8; 32]> {
    // Preserve the existing platform profile for cycle, time, and instret.
    let mut widths = [0; 32];
    widths[..3].fill(64);
    seq!(N in 3..=31 {
        // the caller owns initial discovery, before any PMU consumer.
        #(widths[N] = probe::<N>()?;)*
    });
    Ok(widths)
}

impl Counters<'_> {
    fn check(&self, counter: HardwareCounter) -> Result<()> {
        self.width(counter).map(|_| ())
    }

    /// Writes a counter without changing its running state. RV32 writes use low-zero, high, low.
    pub fn write(&self, counter: HardwareCounter, value: u64) -> Result<()> {
        self.check(counter)?;
        write_counter(counter, value)
    }

    /// Returns accumulation state, or None when hardware lacks inhibit control.
    pub fn is_running(&self, counter: HardwareCounter) -> Result<Option<bool>> {
        self.check(counter)?;
        if self.state.inhibit == InhibitSupport::Unavailable {
            return Ok(None);
        }
        // SAFETY: Runtime entry is installed; this read has no side effects.
        Ok(Some(
            !csrr!(csr::machine::CounterInhibit)
                .map(csr::values::counter::InhibitedCounters::from_bits)?
                .contains(counter),
        ))
    }

    /// Selects a platform-defined event on a discovered programmable counter.
    pub fn configure_event(&self, counter: HpmCounter, event: u64) -> Result<()> {
        self.check(HardwareCounter::Hpm(counter))?;
        write_event(counter, event)
    }

    /// Enables accumulation on a discovered counter.
    pub fn start(&self, counter: HardwareCounter) -> Result<()> {
        self.check(counter)?;
        if self.state.inhibit == InhibitSupport::Available {
            // SAFETY: this discovered counter is controlled in an IRQ scope.
            unsafe {
                csr::machine::CounterInhibit::set(counter, false);
            }
            // SAFETY: the same IRQ scope owns the readback of this inhibit bit.
            if csrr!(csr::machine::CounterInhibit)
                .map(csr::values::counter::InhibitedCounters::from_bits)?
                .contains(counter)
            {
                return Err(Error::NotSupported);
            }
            sync_shadow(counter)?;
        }
        Ok(())
    }

    /// Disables accumulation on a discovered counter.
    /// Hardware without inhibit control keeps its legacy accumulation behavior.
    pub fn stop(&self, counter: HardwareCounter) -> Result<()> {
        self.check(counter)?;
        if self.state.inhibit == InhibitSupport::Available {
            // SAFETY: this discovered counter is controlled in an IRQ scope.
            unsafe {
                csr::machine::CounterInhibit::set(counter, true);
            }
            // SAFETY: the same IRQ scope owns the readback of this inhibit bit.
            if !csrr!(csr::machine::CounterInhibit)
                .map(csr::values::counter::InhibitedCounters::from_bits)?
                .contains(counter)
            {
                return Err(Error::NotSupported);
            }
            sync_shadow(counter)?;
        }
        Ok(())
    }

    pub(super) fn reset_programmable(&self) {
        if self.state.inhibit == InhibitSupport::Available {
            // SAFETY: Runtime holds the hardware lock and excludes local IRQs.
            unsafe {
                machine::CounterInhibit::write(InhibitedCounters::programmable());
            }
        }
    }
}

fn probe<const INDEX: u8>() -> Result<u8> {
    // Event zero stops accumulation even on hardware without mcountinhibit.
    let event = match csrr!(machine::PerformanceEvent<INDEX>) {
        Ok(value) => value,
        Err(Error::IllegalInstruction) => return Ok(0),
        Err(error) => return Err(error),
    };
    #[cfg(target_pointer_width = "32")]
    let event_high = match csrr!(machine::PerformanceEventHigh<INDEX>) {
        Ok(value) => Some(value),
        Err(Error::IllegalInstruction) => None,
        Err(error) => return Err(error),
    };
    let mut previous = None;
    let result = (|| {
        csrw!(machine::PerformanceEvent<INDEX>, 0usize)?;
        #[cfg(target_pointer_width = "32")]
        if event_high.is_some() {
            csrw!(machine::PerformanceEventHigh<INDEX>, 0usize)?;
        }
        previous = Some(read_hpm::<INDEX>()?);
        write_hpm::<INDEX>(u64::MAX)?;
        let implemented = read_hpm::<INDEX>()?;
        Ok((u64::BITS - implemented.leading_zeros()) as u8)
    })();
    // Attempt every restoration, including when a probe access fails.
    let counter_restore = previous.map(write_hpm::<INDEX>).unwrap_or(Ok(()));
    #[cfg(target_pointer_width = "32")]
    let high_restore = event_high
        .map(|value| csrw!(machine::PerformanceEventHigh<INDEX>, value))
        .unwrap_or(Ok(()));
    let event_restore = csrw!(machine::PerformanceEvent<INDEX>, event);
    counter_restore?;
    #[cfg(target_pointer_width = "32")]
    high_restore?;
    event_restore?;
    match result {
        Err(Error::IllegalInstruction) => Ok(0),
        result => result,
    }
}

fn read_hpm<const INDEX: u8>() -> Result<u64> {
    // Discovery selects event zero before reading either counter word.
    #[cfg(target_pointer_width = "32")]
    let high = (csrr!(machine::PerformanceCounterHigh<INDEX>)? as u64) << 32;
    #[cfg(target_pointer_width = "64")]
    let high = 0;
    Ok(high | csrr!(machine::PerformanceCounter<INDEX>)? as u64)
}
pub(super) fn write_counter(counter: HardwareCounter, value: u64) -> Result<()> {
    // the parent checked discovery and excludes local IRQ reentry.
    match counter {
        HardwareCounter::Cycle => {
            #[cfg(target_pointer_width = "32")]
            {
                csrw!(machine::CycleCounter, 0usize)?;
                csrw!(machine::CycleCounterHigh, (value >> 32) as usize)?;
            }
            csrw!(machine::CycleCounter, value as usize)
        }
        HardwareCounter::InstructionsRetired => {
            #[cfg(target_pointer_width = "32")]
            {
                csrw!(machine::InstructionCounter, 0usize)?;
                csrw!(machine::InstructionCounterHigh, (value >> 32) as usize)?;
            }
            csrw!(machine::InstructionCounter, value as usize)
        }
        HardwareCounter::Hpm(counter) => {
            seq!(N in 3..=31 { match counter.index() {
                #(N => write_hpm::<N>(value),)*
                _ => unreachable!("validated HPM counter"),
            } })
        }
    }
}

fn write_hpm<const INDEX: u8>(value: u64) -> Result<()> {
    // the caller owns this discovered counter.
    #[cfg(target_pointer_width = "32")]
    {
        csrw!(machine::PerformanceCounter<INDEX>, 0usize)?;
        csrw!(
            machine::PerformanceCounterHigh<INDEX>,
            (value >> 32) as usize
        )?;
    }
    csrw!(machine::PerformanceCounter<INDEX>, value as usize)
}

pub(super) fn write_event(counter: HpmCounter, event: u64) -> Result<()> {
    // the parent checked discovery and excludes local IRQ reentry.
    seq!(N in 3..=31 { match counter.index() {
        #(N => event_selector::<N>(event),)*
        _ => unreachable!("validated HPM counter"),
    } })
}

fn event_selector<const INDEX: u8>(event: u64) -> Result<()> {
    // the caller owns this discovered programmable selector.
    #[cfg(target_pointer_width = "32")]
    match csrr!(machine::PerformanceEventHigh<INDEX>).map(csr::values::counter::CounterValue::from)
    {
        Ok(_) => csrw!(machine::PerformanceEventHigh<INDEX>, (event >> 32) as usize)?,
        Err(Error::IllegalInstruction) if event >> 32 == 0 => {}
        Err(Error::IllegalInstruction) => return Err(Error::NotSupported),
        Err(error) => return Err(error),
    }
    csrw!(machine::PerformanceEvent<INDEX>, event as usize)
}

/// Preserve the existing machine-read synchronization after CY/IR changes.
pub(super) fn sync_shadow(counter: HardwareCounter) -> Result<()> {
    if matches!(counter, HardwareCounter::Hpm(_)) {
        return Ok(());
    }
    // fixed counters are part of the discovered platform profile.
    #[cfg(target_pointer_width = "32")]
    loop {
        let high = read_fixed_high(counter)?;
        let _ = read_fixed(counter)?;
        if high == read_fixed_high(counter)? {
            break;
        }
    }
    #[cfg(target_pointer_width = "64")]
    let _ = read_fixed(counter)?;
    Ok(())
}

fn read_fixed(counter: HardwareCounter) -> Result<CounterValue> {
    // the caller checked fixed-counter discovery and entry readiness.
    match counter {
        HardwareCounter::Cycle => {
            csrr!(machine::CycleCounter).map(csr::values::counter::CounterValue::from)
        }
        HardwareCounter::InstructionsRetired => {
            csrr!(machine::InstructionCounter).map(csr::values::counter::CounterValue::from)
        }
        HardwareCounter::Hpm(_) => unreachable!("fixed counter"),
    }
}

#[cfg(target_pointer_width = "32")]
fn read_fixed_high(counter: HardwareCounter) -> Result<CounterValue> {
    // the caller checked fixed-counter discovery and entry readiness.
    match counter {
        HardwareCounter::Cycle => {
            csrr!(machine::CycleCounterHigh).map(csr::values::counter::CounterValue::from)
        }
        HardwareCounter::InstructionsRetired => {
            csrr!(machine::InstructionCounterHigh).map(csr::values::counter::CounterValue::from)
        }
        HardwareCounter::Hpm(_) => unreachable!("fixed counter"),
    }
}
