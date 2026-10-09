//! CSR mechanisms for the current hart's performance counters.

use crate::csr::{
    CounterEvent, CounterEventHigh, Csr, Csr64, MachineCounter, MachineCounterHigh, Readable,
    Writable,
};
use crate::trap::Error;

/// Whether hardware can increment a counter during a full-value write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CounterWriteMode {
    /// Inhibition or a zero event selector prevents increments during the write.
    Stopped,
    /// A legacy counter can advance between the RV32 half writes.
    MayRun,
}

/// Detects writable programmable counters and their individual widths.
/// `inhibited` is the read-back value of `mcountinhibit`, or zero when absent.
/// Counters that cannot be inhibited are stopped through their event selectors.
pub(super) fn probe_programmable_counters(inhibited: u32) -> (u32, [u8; 32]) {
    let mut mask = 0;
    let mut widths = [0; 32];
    seq_macro::seq!(N in 3..=31 {
        #(
            if let Some(width) = probe_counter::<
                MachineCounter<N>,
                MachineCounterHigh<N>,
                CounterEvent<N>,
                CounterEventHigh<N>,
            >(inhibited)
                .expect("failed to probe a programmable performance counter")
            {
                mask |= 1 << N;
                widths[N] = width;
            }
        )*
    });
    (mask, widths)
}

fn probe_counter<Low, High, EventLow, EventHigh>(inhibited: u32) -> Result<Option<u8>, Error>
where
    Low: Csr64 + Writable,
    High: Readable<Value = usize> + Writable,
    EventLow: Readable<Value = usize> + Writable,
    EventHigh: Readable<Value = usize> + Writable,
{
    match Low::read64() {
        Ok(_) => {}
        Err(Error::UnsupportedInstruction) => return Ok(None),
        Err(error) => return Err(error),
    }
    if inhibited & (1 << (Low::NUMBER - MachineCounter::<0>::NUMBER)) != 0 {
        return probe_counter_width(Low::read64, |value| {
            write_counter64::<Low, High>(value, CounterWriteMode::Stopped)
        });
    }

    // Event zero means "no event", including on harts without mcountinhibit.
    // Save and restore both event halves so the fallback also preserves RV32
    // Sscofpmf filtering and overflow state.
    let old_event = read_event64::<EventLow, EventHigh>()?;
    let result = write_event64::<EventLow, EventHigh>(0)
        .and_then(|()| read_event64::<EventLow, EventHigh>())
        .and_then(|event| {
            if event != 0 {
                return Err(Error::UnsupportedInstruction);
            }
            probe_counter_width(Low::read64, |value| {
                write_counter64::<Low, High>(value, CounterWriteMode::Stopped)
            })
        });
    write_event64::<EventLow, EventHigh>(old_event)
        .expect("failed to restore a performance counter event after probing");
    assert_eq!(
        read_event64::<EventLow, EventHigh>()
            .expect("failed to verify a restored performance counter event"),
        old_event,
        "performance counter event changed during restoration",
    );
    result
}

fn probe_counter_width(
    mut read_fn: impl FnMut() -> Result<u64, Error>,
    mut write_fn: impl FnMut(u64) -> Result<(), Error>,
) -> Result<Option<u8>, Error> {
    let old_value = read_fn()?;
    let result = write_fn(u64::MAX).and_then(|()| read_fn());
    // Restore both halves even when an RV32 write only modified the high half
    // before failing. Hardware writes do not generate counter-overflow events.
    write_fn(old_value).expect("failed to restore a performance counter after width probing");
    assert_eq!(
        read_fn().expect("failed to verify a restored performance counter"),
        old_value,
        "performance counter changed during restoration",
    );
    result.map(|value| {
        if value == 0 {
            None
        } else {
            Some((u64::BITS - value.leading_zeros()) as u8)
        }
    })
}

fn read_event64<Low, High>() -> Result<u64, Error>
where
    Low: Readable<Value = usize>,
    High: Readable<Value = usize>,
{
    let low = Low::read()? as u64;
    #[cfg(target_pointer_width = "64")]
    {
        Ok(low)
    }
    #[cfg(target_pointer_width = "32")]
    {
        let high = match High::read() {
            Ok(value) => value as u64,
            Err(Error::UnsupportedInstruction) => 0,
            Err(error) => return Err(error),
        };
        Ok(high << 32 | low)
    }
}

pub(super) fn read_counter(index: u8) -> Result<u64, Error> {
    match index {
        0 => MachineCounter::<0>::read64(),
        2 => MachineCounter::<2>::read64(),
        index => seq_macro::seq!(N in 3..=31 {
            match index {
                #(N => MachineCounter::<N>::read64(),)*
                _ => Err(Error::UnsupportedInstruction),
            }
        }),
    }
}

pub(super) fn write_counter(index: u8, value: u64, mode: CounterWriteMode) -> Result<(), Error> {
    match index {
        0 => write_counter64::<MachineCounter<0>, MachineCounterHigh<0>>(value, mode),
        2 => write_counter64::<MachineCounter<2>, MachineCounterHigh<2>>(value, mode),
        index => seq_macro::seq!(N in 3..=31 {
            match index {
                #(N => write_counter64::<MachineCounter<N>, MachineCounterHigh<N>>(value, mode),)*
                _ => Err(Error::UnsupportedInstruction),
            }
        }),
    }
}

fn write_counter64<Low, High>(value: u64, mode: CounterWriteMode) -> Result<(), Error>
where
    Low: Writable<Value = usize>,
    High: Writable<Value = usize>,
{
    #[cfg(target_pointer_width = "64")]
    {
        let _ = mode;
        Low::write(value as usize)
    }
    #[cfg(target_pointer_width = "32")]
    {
        write_counter_halves(
            value,
            mode,
            |low| Low::write(low as usize),
            |high| High::write(high as usize),
        )
    }
}

#[cfg(any(target_pointer_width = "32", test))]
fn write_counter_halves(
    value: u64,
    mode: CounterWriteMode,
    mut write_low_fn: impl FnMut(u32) -> Result<(), Error>,
    mut write_high_fn: impl FnMut(u32) -> Result<(), Error>,
) -> Result<(), Error> {
    if mode == CounterWriteMode::MayRun {
        // Remove a near-overflow old low half before replacing the high half.
        // This does not make a continuously running counter write atomic: the
        // hardware can still advance between the two final CSR writes.
        write_low_fn(0)?;
    }
    write_high_fn((value >> 32) as u32)?;
    write_low_fn(value as u32)
}

pub(super) fn read_event(index: u8) -> Result<u64, Error> {
    seq_macro::seq!(N in 3..=31 {
        match index {
            #(N => read_event64::<CounterEvent<N>, CounterEventHigh<N>>(),)*
            _ => Err(Error::UnsupportedInstruction),
        }
    })
}

pub(super) fn write_event(index: u8, value: u64) -> Result<(), Error> {
    seq_macro::seq!(N in 3..=31 {
        match index {
            #(N => write_event64::<CounterEvent<N>, CounterEventHigh<N>>(value),)*
            _ => Err(Error::UnsupportedInstruction),
        }
    })
}

fn write_event64<Low, High>(value: u64) -> Result<(), Error>
where
    Low: Writable<Value = usize>,
    High: Readable<Value = usize> + Writable,
{
    #[cfg(target_pointer_width = "32")]
    match High::read() {
        Ok(_) => High::write((value >> 32) as usize)?,
        // RV32 event-selector high halves require an extension such as
        // Sscofpmf. A selector whose upper half is zero also fits the base CSR.
        Err(Error::UnsupportedInstruction) if value >> 32 == 0 => {}
        Err(error) => return Err(error),
    }
    Low::write(value as usize)
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use super::{CounterWriteMode, Error, probe_counter_width, write_counter_halves};

    #[test]
    fn width_probe_preserves_values_for_narrow_and_full_counters() {
        for width in [32, 40, 64] {
            let mask = u64::MAX >> (u64::BITS - width);
            let original = 0x1234_5678_9abc_def0 & mask;
            let value = Cell::new(original);
            let result = probe_counter_width(
                || Ok(value.get()),
                |new_value| {
                    value.set(new_value & mask);
                    Ok(())
                },
            );
            assert_eq!(result, Ok(Some(width as u8)));
            assert_eq!(value.get(), original);
        }
    }

    #[test]
    fn hardwired_zero_counter_has_no_width() {
        assert_eq!(probe_counter_width(|| Ok(0), |_| Ok(())), Ok(None));
    }

    #[test]
    fn failed_partial_write_still_restores_the_full_value() {
        let original = 0x1234_5678_9abc_def0;
        let value = Cell::new(original);
        let writes = Cell::new(0);
        let result = probe_counter_width(
            || Ok(value.get()),
            |new_value| {
                let count = writes.get();
                writes.set(count + 1);
                if count == 0 {
                    // Model an RV32 write that changes the high half before
                    // the low-half access fails.
                    value.set((new_value & !0xffff_ffff) | (value.get() & 0xffff_ffff));
                    Err(Error::UnsupportedInstruction)
                } else {
                    value.set(new_value);
                    Ok(())
                }
            },
        );
        assert_eq!(result, Err(Error::UnsupportedInstruction));
        assert_eq!(value.get(), original);
        assert_eq!(writes.get(), 2);
    }

    #[test]
    fn running_counter_write_clears_the_old_carry_window() {
        let target = 0x1234_5678_0000_0042;
        let value = Cell::new(0xdead_beef_ffff_ffffu64);
        let steps = Cell::new(0);
        let result = write_counter_halves(
            target,
            CounterWriteMode::MayRun,
            |low| {
                let step = steps.get();
                if step == 0 {
                    assert_eq!(low, 0);
                } else {
                    assert_eq!(step, 2);
                }
                steps.set(step + 1);
                value.set((value.get() & !0xffff_ffff) | u64::from(low));
                if step == 0 {
                    value.set(value.get().wrapping_add(1));
                }
                Ok(())
            },
            |high| {
                assert_eq!(steps.get(), 1);
                steps.set(2);
                value.set((u64::from(high) << 32) | (value.get() & 0xffff_ffff));
                value.set(value.get().wrapping_add(1));
                Ok(())
            },
        );
        assert_eq!(result, Ok(()));
        assert_eq!(value.get(), target);
        assert_eq!(steps.get(), 3);
    }

    #[test]
    fn stopped_counter_write_preserves_the_two_write_sequence() {
        let steps = Cell::new(0);
        assert_eq!(
            write_counter_halves(
                0x1234_5678_9abc_def0,
                CounterWriteMode::Stopped,
                |low| {
                    assert_eq!(steps.get(), 1);
                    assert_eq!(low, 0x9abc_def0);
                    steps.set(2);
                    Ok(())
                },
                |high| {
                    assert_eq!(steps.get(), 0);
                    assert_eq!(high, 0x1234_5678);
                    steps.set(1);
                    Ok(())
                },
            ),
            Ok(())
        );
        assert_eq!(steps.get(), 2);
    }

    #[test]
    fn failed_counter_prefix_write_keeps_the_access_error() {
        assert_eq!(
            write_counter_halves(
                7,
                CounterWriteMode::MayRun,
                |_| Err(Error::UnsupportedInstruction),
                |_| panic!("high half must not be written after a failed prefix"),
            ),
            Err(Error::UnsupportedInstruction)
        );
    }
}
