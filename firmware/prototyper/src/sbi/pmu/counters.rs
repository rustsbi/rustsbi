//! One hart's logical counters, bindings, and firmware event counts.

use super::SbiPmu;
use super::event::{Event, HardwareEvents};
use runtime::hart::HartId;
use runtime::pmu::{Counters as HardwareCounters, HardwareCounter};
use runtime::{Error, Result};
use sbi_spec::pmu::{
    flags::{ConfigFlags, StopFlags},
    hardware_event,
};

const HARDWARE_COUNTERS: usize = 32;
const FIRMWARE_COUNTERS: usize = 16;

impl SbiPmu {
    pub(super) fn with_current<T>(
        &self,
        operation: impl FnOnce(&HardwareCounters<'_>, &mut CounterTable) -> Result<T>,
    ) -> Result<T> {
        self.pmu.with_current(|hardware| {
            let mut local = self
                .counters
                .get(HartId::current()?)?
                .try_lock()
                .ok_or(Error::Busy)?;
            let counters = local.get_or_insert_with(|| CounterTable::new(hardware.implemented()));
            operation(hardware, counters)
        })
    }

    pub(crate) fn reset_records(&self) -> Result<()> {
        *self
            .counters
            .get(HartId::current()?)?
            .try_lock()
            .ok_or(Error::Busy)? = None;
        Ok(())
    }
}

pub(super) struct CounterTable {
    counters: [Counter; HARDWARE_COUNTERS + FIRMWARE_COUNTERS],
    count: usize,
}

#[derive(Clone, Copy)]
enum Counter {
    Hardware {
        counter: HardwareCounter,
        event: Option<Event>,
    },
    /// Time is enumerated for ABI compatibility, but cannot be started or stopped.
    Time,
    Firmware {
        event: Option<Event>,
        value: u64,
        running: bool,
    },
}

impl Counter {
    fn event(self) -> Option<Event> {
        match self {
            Self::Hardware { event, .. } | Self::Firmware { event, .. } => event,
            Self::Time => None,
        }
    }

    fn is_running(self, hardware: &HardwareCounters<'_>) -> Result<bool> {
        match self {
            Self::Hardware { counter, .. } => Ok(hardware.is_running(counter)? == Some(true)),
            Self::Firmware { running, .. } => Ok(running),
            Self::Time => Err(Error::InvalidArgs),
        }
    }
}

/// Validated logical-counter indices selected for one SBI operation.
#[derive(Clone, Copy)]
pub(super) struct CounterSet {
    remaining: u64,
}

impl Iterator for CounterSet {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        if self.remaining == 0 {
            return None;
        }
        let index = self.remaining.trailing_zeros() as usize;
        self.remaining &= self.remaining - 1;
        Some(index)
    }
}

impl CounterTable {
    pub(super) fn new(implemented: u32) -> Self {
        let mut table = Self {
            counters: [Counter::Time; HARDWARE_COUNTERS + FIRMWARE_COUNTERS],
            count: 0,
        };
        for position in 0..32 {
            if implemented & (1 << position) == 0 {
                continue;
            }
            table.counters[table.count] = match HardwareCounter::from_index(position) {
                Some(counter) => Counter::Hardware {
                    counter,
                    event: match counter {
                        HardwareCounter::Cycle => {
                            Some(Event::try_from(hardware_event::CPU_CYCLES).unwrap())
                        }
                        HardwareCounter::InstructionsRetired => {
                            Some(Event::try_from(hardware_event::INSTRUCTIONS).unwrap())
                        }
                        HardwareCounter::Hpm(_) => None,
                    },
                },
                None => Counter::Time,
            };
            table.count += 1;
        }
        for counter in &mut table.counters[table.count..table.count + FIRMWARE_COUNTERS] {
            *counter = Counter::Firmware {
                event: None,
                value: 0,
                running: false,
            };
        }
        table.count += FIRMWARE_COUNTERS;
        table
    }

    pub(super) fn count(&self) -> usize {
        self.count
    }

    pub(super) fn select(&self, first: usize, offsets: usize) -> Result<CounterSet> {
        if first >= self.count
            || offsets == 0
            || offsets
                .checked_shr((self.count - first) as u32)
                .unwrap_or(0)
                != 0
        {
            return Err(Error::InvalidArgs);
        }
        Ok(CounterSet {
            remaining: (offsets as u64) << first,
        })
    }

    pub(super) fn info(&self, index: usize, hardware: &HardwareCounters<'_>) -> Result<usize> {
        if index >= self.count {
            return Err(Error::InvalidArgs);
        }
        Ok(match self.counters[index] {
            Counter::Hardware { counter, .. } => {
                usize::from(counter.user_csr()) | (usize::from(hardware.width(counter)? - 1) << 12)
            }
            Counter::Time => 0xc01 | (63 << 12),
            Counter::Firmware { .. } => 1 << (usize::BITS - 1),
        })
    }

    pub(super) fn configure(
        &mut self,
        selection: CounterSet,
        flags: ConfigFlags,
        event: Event,
        data: u64,
        events: &HardwareEvents,
        hardware: &HardwareCounters<'_>,
    ) -> Result<usize> {
        let skip = flags.contains(ConfigFlags::SKIP_MATCH);
        let index = if skip {
            selection.into_iter().next().unwrap()
        } else {
            self.find(selection, event, data, events, hardware)?
        };
        let event = if skip {
            self.counters[index].event().ok_or(Error::InvalidArgs)?
        } else {
            event
        };
        let clear = flags.contains(ConfigFlags::CLEAR_VALUE);
        let start = flags.contains(ConfigFlags::AUTO_START);
        match &mut self.counters[index] {
            Counter::Hardware {
                counter,
                event: binding,
            } => {
                if start && hardware.is_running(*counter)? == Some(true) {
                    return Err(Error::NotSupported);
                }
                if !skip && let HardwareCounter::Hpm(counter) = counter {
                    hardware.configure_event(*counter, events.encode(event, data)?)?;
                }
                if clear {
                    hardware.write(*counter, 0)?;
                }
                if start {
                    hardware.start(*counter)?;
                }
                *binding = Some(event);
            }
            Counter::Firmware {
                event: binding,
                value,
                running,
            } => {
                if clear {
                    *value = 0;
                }
                if start {
                    *running = true;
                }
                *binding = Some(event);
            }
            Counter::Time => return Err(Error::InvalidArgs),
        }
        Ok(index)
    }

    fn find(
        &self,
        selection: CounterSet,
        event: Event,
        data: u64,
        events: &HardwareEvents,
        hardware: &HardwareCounters<'_>,
    ) -> Result<usize> {
        let eligible = events.counters(event, data);
        let mut fixed = None;
        for index in selection {
            match self.counters[index] {
                Counter::Firmware {
                    event: None,
                    running: false,
                    ..
                } if event.is_firmware() => return Ok(index),
                Counter::Hardware {
                    counter,
                    event: binding,
                } if !event.is_firmware() => {
                    if event.fixed_counter() == Some(counter) {
                        fixed = Some(index);
                    }
                    if matches!(counter, HardwareCounter::Hpm(_))
                        && binding.is_none()
                        && eligible & (1 << counter.index()) != 0
                        && hardware.is_running(counter)? != Some(true)
                    {
                        return Ok(index);
                    }
                }
                _ => {}
            }
        }
        fixed.ok_or(Error::NotSupported)
    }

    pub(super) fn start(
        &mut self,
        selection: CounterSet,
        initial: Option<u64>,
        hardware: &HardwareCounters<'_>,
    ) -> Result<()> {
        // Validate the entire set before changing any value or running state.
        for index in selection {
            if self.counters[index].event().is_none() {
                return Err(Error::InvalidArgs);
            }
            if self.counters[index].is_running(hardware)? {
                return Err(Error::AlreadyStarted);
            }
        }
        for index in selection {
            match &mut self.counters[index] {
                Counter::Hardware { counter, .. } => {
                    if let Some(initial) = initial {
                        hardware.write(*counter, initial)?;
                    }
                    hardware.start(*counter)?;
                }
                Counter::Firmware { value, running, .. } => {
                    if let Some(initial) = initial {
                        *value = initial;
                    }
                    *running = true;
                }
                Counter::Time => unreachable!("selection was validated"),
            }
        }
        Ok(())
    }

    pub(super) fn stop(
        &mut self,
        selection: CounterSet,
        flags: StopFlags,
        hardware: &HardwareCounters<'_>,
    ) -> Result<()> {
        let mut stopped = false;
        let mut already_stopped = false;
        let reset = flags.contains(StopFlags::RESET);
        for index in selection {
            match &mut self.counters[index] {
                Counter::Hardware { counter, event } => {
                    if hardware.is_running(*counter)? == Some(false) {
                        already_stopped = true;
                    } else {
                        hardware.stop(*counter)?;
                    }
                    if reset {
                        if let HardwareCounter::Hpm(counter) = counter {
                            hardware.configure_event(*counter, 0)?;
                        }
                        *event = None;
                    }
                }
                Counter::Firmware { event, running, .. } => {
                    if !*running {
                        already_stopped = true;
                    }
                    *running = false;
                    if reset {
                        *event = None;
                    }
                }
                Counter::Time => continue,
            }
            stopped = true;
        }
        if already_stopped {
            Err(Error::AlreadyStopped)
        } else if stopped {
            Ok(())
        } else {
            Err(Error::InvalidArgs)
        }
    }

    pub(super) fn read_firmware(&self, index: usize) -> Result<u64> {
        if index >= self.count {
            return Err(Error::InvalidArgs);
        }
        match self.counters[index] {
            Counter::Firmware {
                event: Some(_),
                value,
                ..
            } => Ok(value),
            _ => Err(Error::InvalidArgs),
        }
    }

    pub(super) fn record(&mut self, observed: Event) {
        for counter in &mut self.counters[..self.count] {
            if let Counter::Firmware {
                event: Some(event),
                value,
                running: true,
            } = counter
                && *event == observed
            {
                *value = value.wrapping_add(1);
            }
        }
    }
}
