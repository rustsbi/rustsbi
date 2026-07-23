//! Transactional counter, inhibit, and event-selector operations.

use super::hart::*;
use super::riscv::csr::*;

pub(super) fn reset_all(facts: HartCounters) -> Result<(), CounterError> {
    let programmable = facts.controllable & !0b111;
    if programmable != 0 {
        replace_inhibit(|value| value | programmable as usize)?;
        let actual = read_inhibit()?;
        if actual & programmable as usize != programmable as usize {
            return Err(CounterError::MechanismFailure);
        }
        for offset in FIRST_PROGRAMMABLE_OFFSET..=LAST_COUNTER_OFFSET {
            if programmable & (1u32 << offset) != 0 {
                write_event(offset, 0, facts.event_is_wide(offset))?;
                write_counter(offset, 0)?;
            }
        }
    }
    // Cycle and retired-instruction counters form the deterministic running
    // baseline. `time` is not controlled by this register.
    let fixed = facts.controllable & 0b101;
    if fixed != 0 {
        replace_inhibit(|value| value & !(fixed as usize))?;
        if read_inhibit()? & fixed as usize != 0 {
            return Err(CounterError::MechanismFailure);
        }
    }
    Ok(())
}

pub(super) fn require_stopped(offset: u8) -> Result<(), CounterError> {
    let bit = 1usize << offset;
    if read_inhibit()? & bit == 0 {
        Err(CounterError::AlreadyStarted)
    } else {
        Ok(())
    }
}

pub(super) fn require_started(offset: u8) -> Result<(), CounterError> {
    let bit = 1usize << offset;
    if read_inhibit()? & bit != 0 {
        Err(CounterError::AlreadyStopped)
    } else {
        Ok(())
    }
}

pub(super) fn set_inhibited(offset: u8, inhibited: bool) -> Result<(), CounterError> {
    let bit = 1usize << offset;
    replace_inhibit(|value| if inhibited { value | bit } else { value & !bit })?;
    if (read_inhibit()? & bit != 0) == inhibited {
        Ok(())
    } else {
        Err(CounterError::MechanismFailure)
    }
}

pub(super) fn replace_inhibit(update: impl FnOnce(usize) -> usize) -> Result<(), CounterError> {
    let current = read_inhibit()?;
    match expected_swap::<0x320>(update(current)) {
        LowResult::Value(_) => Ok(()),
        LowResult::Illegal | LowResult::Failure => Err(CounterError::MechanismFailure),
    }
}

pub(super) fn read_inhibit() -> Result<usize, CounterError> {
    match expected_read::<0x320>() {
        LowResult::Value(value) => Ok(value),
        LowResult::Illegal | LowResult::Failure => Err(CounterError::MechanismFailure),
    }
}

pub(super) fn read_counter(offset: u8) -> Result<u64, CounterError> {
    #[cfg(target_pointer_width = "64")]
    {
        low_value(read_counter_low(offset)).map(|value| value as u64)
    }
    #[cfg(target_pointer_width = "32")]
    loop {
        let high_before = low_value(read_counter_high(offset))?;
        let low = low_value(read_counter_low(offset))?;
        let high_after = low_value(read_counter_high(offset))?;
        if high_before == high_after {
            return Ok(((high_after as u64) << 32) | low as u64);
        }
    }
}

pub(super) fn write_counter(offset: u8, value: u64) -> Result<(), CounterError> {
    #[cfg(target_pointer_width = "64")]
    {
        low_value(write_counter_low(offset, value as usize))?;
    }
    #[cfg(target_pointer_width = "32")]
    {
        // The counter is inhibited before this operation, so writing the high
        // half first cannot race an increment or expose a torn running value.
        low_value(write_counter_high(offset, (value >> 32) as usize))?;
        low_value(write_counter_low(offset, value as usize))?;
    }
    if read_counter(offset)? != value {
        return Err(CounterError::MechanismFailure);
    }
    Ok(())
}

pub(super) fn write_event(offset: u8, value: u64, wide: bool) -> Result<(), CounterError> {
    if !(FIRST_PROGRAMMABLE_OFFSET..=LAST_COUNTER_OFFSET).contains(&offset) {
        return Err(CounterError::UnsupportedEvent);
    }
    #[cfg(target_pointer_width = "64")]
    {
        let _ = wide;
        low_value(write_event_low(offset, value as usize))?;
        if low_value(read_event_low(offset))? as u64 != value {
            return Err(CounterError::UnsupportedEvent);
        }
    }
    #[cfg(target_pointer_width = "32")]
    {
        if value >> 32 != 0 && !wide {
            return Err(CounterError::UnsupportedEvent);
        }
        if wide {
            low_value(write_event_high(offset, (value >> 32) as usize))?;
        }
        low_value(write_event_low(offset, value as usize))?;
        let high = if wide {
            low_value(read_event_high(offset))? as u64
        } else {
            0
        };
        let actual = (high << 32) | low_value(read_event_low(offset))? as u64;
        if actual != value {
            return Err(CounterError::UnsupportedEvent);
        }
    }
    Ok(())
}

pub(super) fn read_event(offset: u8, wide: bool) -> Result<u64, CounterError> {
    if !(FIRST_PROGRAMMABLE_OFFSET..=LAST_COUNTER_OFFSET).contains(&offset) {
        return Err(CounterError::UnsupportedEvent);
    }
    #[cfg(target_pointer_width = "64")]
    {
        let _ = wide;
        low_value(read_event_low(offset)).map(|value| value as u64)
    }
    #[cfg(target_pointer_width = "32")]
    {
        let high = if wide {
            low_value(read_event_high(offset))?
        } else {
            0
        };
        let low = low_value(read_event_low(offset))?;
        Ok(((high as u64) << 32) | low as u64)
    }
}

pub(super) fn restore_or_abort(result: Result<(), CounterError>) {
    if result.is_err() {
        // A typed operation may report failure only if the original state was
        // restored. Continuing after a failed repair would make the safe
        // counter capability's software-visible state disagree with hardware.
        crate::power::abort(|| {})
    }
}
