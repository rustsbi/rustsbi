//! One hart's logical counters, bindings, and firmware event counts.

use super::SbiPmu;
use super::event::{Event, HardwareEvents};
use runtime::hart::HartId;
use runtime::pmu::{Counters as HardwareCounters, HardwareCounter, Pmu};
use runtime::{Error, Result};
use sbi_spec::pmu::flags::{ConfigFlags, StopFlags};

const HARDWARE_COUNTERS: usize = 32;
const FIRMWARE_COUNTERS: usize = 16;

impl SbiPmu {
    pub(super) fn with_current<T>(
        &self,
        operation: impl FnOnce(&HardwareCounters<'_>, &mut CounterTable) -> Result<T>,
    ) -> Result<T> {
        Pmu::with_current(|hardware| {
            let mut record = self
                .counters
                .get(HartId::current()?)?
                .try_lock()
                .ok_or(Error::Busy)?;
            let counters = record.get_or_insert_with(|| CounterTable::new(hardware.implemented()));
            operation(hardware, counters)
        })
    }

    pub(crate) fn reset_current_hart_records(&self) -> Result<()> {
        let mut record = self
            .counters
            .get(HartId::current()?)?
            .try_lock()
            .ok_or(Error::Busy)?;
        *record = None;
        Ok(())
    }
}

pub(super) struct CounterTable {
    counters: [Counter; HARDWARE_COUNTERS + FIRMWARE_COUNTERS],
    count: usize,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum CounterState {
    Stopped,
    Running,
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
        state: CounterState,
    },
}

impl Counter {
    fn from_architectural(index: u16) -> Self {
        match HardwareCounter::from_index(index) {
            Some(counter) => Self::Hardware {
                counter,
                event: match counter {
                    HardwareCounter::Cycle => Some(Event::CPU_CYCLES),
                    HardwareCounter::InstructionsRetired => Some(Event::INSTRUCTIONS),
                    HardwareCounter::Hpm(_) => None,
                },
            },
            None => Self::Time,
        }
    }

    fn event(self) -> Option<Event> {
        match self {
            Self::Hardware { event, .. } | Self::Firmware { event, .. } => event,
            Self::Time => None,
        }
    }

    /// Time is enumerated as a hardware counter, matching OpenSBI's class split.
    fn is_firmware(self) -> bool {
        matches!(self, Self::Firmware { .. })
    }

    fn is_running(self, hardware: &HardwareCounters<'_>) -> Result<bool> {
        match self {
            Self::Hardware { counter, .. } => Ok(hardware.is_running(counter)? == Some(true)),
            Self::Firmware { state, .. } => Ok(state == CounterState::Running),
            Self::Time => Err(Error::InvalidArgs),
        }
    }
}

/// Validated logical-counter indices selected for one SBI operation.
#[derive(Clone, Copy)]
pub(super) struct CounterSet {
    remaining: u64,
}

impl CounterSet {
    fn first(self) -> Option<usize> {
        let remaining = self.remaining;
        (remaining != 0).then(|| remaining.trailing_zeros() as usize)
    }
}

impl Iterator for CounterSet {
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        let index = self.first()?;
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
            table.counters[table.count] = Counter::from_architectural(position);
            table.count += 1;
        }
        for counter in &mut table.counters[table.count..table.count + FIRMWARE_COUNTERS] {
            *counter = Counter::Firmware {
                event: None,
                value: 0,
                state: CounterState::Stopped,
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
            selection.first().ok_or(Error::InvalidArgs)?
        } else {
            self.find(selection, event, data, events, hardware)?
        };
        let event = if skip {
            let counter = self.counters[index];
            // Match OpenSBI: a selected counter of the other class cannot
            // monitor this event. Reject before touching any counter state.
            if counter.is_firmware() != event.is_firmware() {
                return Err(Error::NotSupported);
            }
            counter.event().ok_or(Error::InvalidArgs)?
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
                state,
            } => {
                if clear {
                    *value = 0;
                }
                if start {
                    *state = CounterState::Running;
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
                    state: CounterState::Stopped,
                    ..
                } if event.is_firmware() => return Ok(index),
                Counter::Hardware {
                    counter,
                    event: binding,
                } if !event.is_firmware() => {
                    if event.fixed_counter() == Some(counter) {
                        fixed = Some(index);
                    }
                    if Self::can_monitor(counter, binding, eligible, hardware)? {
                        return Ok(index);
                    }
                }
                _ => {}
            }
        }
        fixed.ok_or(Error::NotSupported)
    }

    /// Checks whether a free programmable counter can monitor this event now.
    fn can_monitor(
        counter: HardwareCounter,
        binding: Option<Event>,
        eligible: u32,
        hardware: &HardwareCounters<'_>,
    ) -> Result<bool> {
        if !matches!(counter, HardwareCounter::Hpm(_)) || binding.is_some() {
            return Ok(false);
        }
        if eligible & (1 << counter.index()) == 0 {
            return Ok(false);
        }
        Ok(hardware.is_running(counter)? != Some(true))
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
                Counter::Firmware { value, state, .. } => {
                    if let Some(initial) = initial {
                        *value = initial;
                    }
                    *state = CounterState::Running;
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
                        // A stopped counter keeps its binding even when RESET
                        // is requested; other counters in the batch still stop.
                        already_stopped = true;
                        continue;
                    }
                    hardware.stop(*counter)?;
                    if reset {
                        if let HardwareCounter::Hpm(counter) = counter {
                            hardware.configure_event(*counter, 0)?;
                        }
                        *event = None;
                    }
                }
                Counter::Firmware { event, state, .. } => {
                    if *state == CounterState::Stopped {
                        already_stopped = true;
                        continue;
                    }
                    *state = CounterState::Stopped;
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
                state: CounterState::Running,
            } = counter
                && *event == observed
            {
                *value = value.wrapping_add(1);
            }
        }
    }
}
