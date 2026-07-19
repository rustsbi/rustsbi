//! Per-hart discovery of implemented and controllable counters.

use super::arch::*;
use super::control::read_inhibit;
use super::state::*;

pub(super) fn probe_current() -> Result<CounterFacts, CounterError> {
    let mut accessible = 0u32;
    for offset in 0..=LAST_COUNTER_OFFSET {
        if offset == 1 {
            continue;
        }
        match probe_counter(offset) {
            Ok(true) => accessible |= 1u32 << offset,
            Ok(false) => {}
            Err(error) => return Err(error),
        }
    }
    let controllable = probe_controllable(accessible)?;
    let wide_events = probe_wide_events(controllable)?;
    Ok(CounterFacts {
        accessible,
        controllable,
        wide_events,
        initialized: true,
    })
}

#[cfg(target_pointer_width = "64")]
pub(super) fn probe_wide_events(controllable: u32) -> Result<u32, CounterError> {
    Ok(controllable & !0b111)
}

#[cfg(target_pointer_width = "32")]
pub(super) fn probe_wide_events(controllable: u32) -> Result<u32, CounterError> {
    let mut wide = 0u32;
    for offset in FIRST_PROGRAMMABLE_OFFSET..=LAST_COUNTER_OFFSET {
        if controllable & (1u32 << offset) == 0 {
            continue;
        }
        match read_event_high(offset) {
            LowResult::Value(_) => wide |= 1u32 << offset,
            // The high event-selector CSR is optional on RV32. Its absence
            // narrows this counter's accepted event data to 32 bits.
            LowResult::Illegal => {}
            LowResult::Failure => return Err(CounterError::MechanismFailure),
        }
    }
    Ok(wide)
}

pub(super) fn probe_controllable(implemented: u32) -> Result<u32, CounterError> {
    if implemented == 0 {
        return Ok(0);
    }
    let original = match expected_read::<0x320>() {
        LowResult::Value(value) => value,
        LowResult::Illegal => return Ok(0),
        LowResult::Failure => return Err(CounterError::MechanismFailure),
    };
    let requested = original | implemented as usize;
    match expected_swap::<0x320>(requested) {
        LowResult::Value(_) => {}
        LowResult::Illegal => return Ok(0),
        LowResult::Failure => return Err(CounterError::MechanismFailure),
    }
    let actual = read_inhibit()?;
    match expected_swap::<0x320>(original) {
        LowResult::Value(_) if read_inhibit()? == original => {}
        LowResult::Value(_) | LowResult::Illegal | LowResult::Failure => {
            return Err(CounterError::MechanismFailure);
        }
    }
    Ok((actual & implemented as usize) as u32)
}

pub(super) fn probe_counter(offset: u8) -> Result<bool, CounterError> {
    match read_counter_low(offset) {
        LowResult::Value(_) => {}
        LowResult::Illegal => return Ok(false),
        LowResult::Failure => return Err(CounterError::MechanismFailure),
    }
    #[cfg(target_pointer_width = "32")]
    match read_counter_high(offset) {
        LowResult::Value(_) => {}
        LowResult::Illegal => return Ok(false),
        LowResult::Failure => return Err(CounterError::MechanismFailure),
    }
    Ok(true)
}
