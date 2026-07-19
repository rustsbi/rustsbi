//! Performance-monitoring SBI policy.

use alloc::vec;

use machine::{CounterError, CounterId, HartLocal, HartLocalError, PerformanceCounters};
use rustsbi::SbiRet;
use sbi_spec::pmu::{
    cache_event, cache_operation, cache_result, event_type, firmware_event, flags, hardware_event,
};

const HARDWARE_COUNTER_LIMIT: usize = 32;
const FIRMWARE_COUNTER_COUNT: usize = 16;
const COUNTER_LIMIT: usize = HARDWARE_COUNTER_LIMIT + FIRMWARE_COUNTER_COUNT;
const EVENT_INDEX_MASK: usize = 0x000f_ffff;

/// Upper SBI service that assigns architectural and firmware events to the
/// calling hart's counters.
pub(super) struct PerformanceMonitor {
    counters: PerformanceCounters,
    state: HartLocal<HartState>,
}

#[derive(Clone, Copy)]
struct HartState {
    initialized: bool,
    assignments: [Option<Assignment>; COUNTER_LIMIT],
    firmware_values: [u64; FIRMWARE_COUNTER_COUNT],
}

#[derive(Clone, Copy)]
enum Assignment {
    Hardware {
        counter: CounterId,
        event: Event,
        running: bool,
    },
    Firmware {
        event_code: usize,
        running: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Event {
    index: usize,
    selector: u64,
    kind: EventKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EventKind {
    Hardware,
    Firmware(usize),
}

impl HartState {
    const NEW: Self = Self {
        initialized: false,
        assignments: [None; COUNTER_LIMIT],
        firmware_values: [0; FIRMWARE_COUNTER_COUNT],
    };

    fn initialize_fixed(&mut self, counters: &PerformanceCounters) -> Result<(), CounterError> {
        if self.initialized {
            return Ok(());
        }
        for index in 0..counters.count() {
            let counter = counters
                .counter(index)
                .ok_or(CounterError::MechanismFailure)?;
            let info = counters.info(counter)?;
            let event_index = match info.csr_number() {
                0x0c00 => Some(hardware_event::CPU_CYCLES),
                0x0c02 => Some(hardware_event::INSTRUCTIONS),
                _ => None,
            };
            if let Some(event_index) = event_index {
                self.assignments[index] = Some(Assignment::Hardware {
                    counter,
                    event: Event {
                        index: event_index,
                        selector: event_index as u64,
                        kind: EventKind::Hardware,
                    },
                    running: true,
                });
            }
        }
        self.initialized = true;
        Ok(())
    }
}

impl PerformanceMonitor {
    pub(super) fn new(
        counters: PerformanceCounters,
        hart_count: usize,
    ) -> Result<Self, HartLocalError> {
        Ok(Self {
            counters,
            state: HartLocal::new(vec![HartState::NEW; hart_count])?,
        })
    }

    pub(super) fn record(&self, event_code: usize) {
        let Ok(mut state) = self.state.current() else {
            return;
        };
        let hardware_count = self.counters.count();
        for index in 0..FIRMWARE_COUNTER_COUNT {
            if matches!(
                state.assignments[hardware_count + index],
                Some(Assignment::Firmware {
                    event_code: assigned,
                    running: true,
                }) if assigned == event_code
            ) {
                state.firmware_values[index] = state.firmware_values[index].wrapping_add(1);
            }
        }
    }

    fn total_counters(&self) -> usize {
        self.counters.count() + FIRMWARE_COUNTER_COUNT
    }

    fn current_state(&self) -> Result<machine::HartLocalGuard<'_, HartState>, SbiRet> {
        let mut state = self.state.current().map_err(|_| SbiRet::failed())?;
        state
            .initialize_fixed(&self.counters)
            .map_err(counter_error)?;
        Ok(state)
    }

    fn configure_new_hardware(
        &self,
        index: usize,
        event: Event,
        flags: flags::CounterCfgFlags,
        state: &mut HartState,
    ) -> Result<(), SbiRet> {
        let counter = self
            .counters
            .counter(index)
            .ok_or_else(SbiRet::invalid_param)?;
        if state.assignments[index].is_some() {
            return Err(SbiRet::not_supported());
        }
        self.counters
            .configure(counter, event.index, event.selector)
            .map_err(counter_error)?;
        let running = apply_hardware_config(&self.counters, counter, flags)?;
        state.assignments[index] = Some(Assignment::Hardware {
            counter,
            event,
            running,
        });
        Ok(())
    }

    fn configure_existing(
        &self,
        index: usize,
        event: Event,
        flags: flags::CounterCfgFlags,
        state: &mut HartState,
    ) -> Result<(), SbiRet> {
        match state.assignments[index] {
            Some(Assignment::Hardware {
                counter,
                event: assigned,
                running: false,
            }) if assigned == event => {
                let running = apply_hardware_config(&self.counters, counter, flags)?;
                state.assignments[index] = Some(Assignment::Hardware {
                    counter,
                    event,
                    running,
                });
                Ok(())
            }
            Some(Assignment::Firmware {
                event_code,
                running: false,
            }) if event.kind == EventKind::Firmware(event_code) => {
                let firmware_index = index - self.counters.count();
                if flags.contains(flags::CounterCfgFlags::CLEAR_VALUE) {
                    state.firmware_values[firmware_index] = 0;
                }
                state.assignments[index] = Some(Assignment::Firmware {
                    event_code,
                    running: flags.contains(flags::CounterCfgFlags::AUTO_START),
                });
                Ok(())
            }
            _ => Err(SbiRet::invalid_param()),
        }
    }

    fn rollback_started(
        &self,
        state: &mut HartState,
        selection: CounterSelection,
        until: usize,
        values: &[Option<u64>; COUNTER_LIMIT],
    ) {
        for index in selection.take(until) {
            if let Some(Assignment::Hardware {
                counter,
                event,
                running: true,
            }) = state.assignments[index]
            {
                if self.counters.stop(counter).is_err()
                    || values[index]
                        .is_none_or(|value| self.counters.set_value(counter, value).is_err())
                {
                    machine::abort(|| {});
                }
                state.assignments[index] = Some(Assignment::Hardware {
                    counter,
                    event,
                    running: false,
                });
            } else if let Some(Assignment::Firmware {
                event_code,
                running: true,
            }) = state.assignments[index]
            {
                if let Some(value) = values[index] {
                    state.firmware_values[index - self.counters.count()] = value;
                }
                state.assignments[index] = Some(Assignment::Firmware {
                    event_code,
                    running: false,
                });
            }
        }
    }

    fn rollback_stopped(
        &self,
        state: &mut HartState,
        selection: CounterSelection,
        until: usize,
        values: &[Option<u64>; COUNTER_LIMIT],
        reset: bool,
    ) {
        for index in selection.take(until) {
            match state.assignments[index] {
                Some(Assignment::Hardware {
                    counter,
                    event,
                    running: false,
                }) => {
                    if reset
                        && (self
                            .counters
                            .configure(counter, event.index, event.selector)
                            .is_err()
                            || values[index].is_none_or(|value| {
                                self.counters.set_value(counter, value).is_err()
                            }))
                    {
                        machine::abort(|| {});
                    }
                    if self.counters.start(counter, None).is_err() {
                        machine::abort(|| {});
                    }
                    state.assignments[index] = Some(Assignment::Hardware {
                        counter,
                        event,
                        running: true,
                    });
                }
                Some(Assignment::Firmware {
                    event_code,
                    running: false,
                }) => {
                    state.assignments[index] = Some(Assignment::Firmware {
                        event_code,
                        running: true,
                    });
                }
                _ => {}
            }
        }
    }
}

impl rustsbi::Pmu for PerformanceMonitor {
    fn num_counters(&self) -> usize {
        self.total_counters()
    }

    fn counter_get_info(&self, counter_idx: usize) -> SbiRet {
        let hardware_count = self.counters.count();
        if counter_idx < hardware_count {
            let Some(counter) = self.counters.counter(counter_idx) else {
                return SbiRet::invalid_param();
            };
            return match self.counters.info(counter) {
                Ok(info) => SbiRet::success(
                    usize::from(info.csr_number()) | (usize::from(info.width() - 1) << 12),
                ),
                Err(error) => counter_error(error),
            };
        }
        if counter_idx < self.total_counters() {
            SbiRet::success(1usize << (usize::BITS - 1))
        } else {
            SbiRet::invalid_param()
        }
    }

    fn counter_config_matching(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        config_flags: usize,
        event_idx: usize,
        event_data: u64,
    ) -> SbiRet {
        let Some(flags) = flags::CounterCfgFlags::from_bits(config_flags) else {
            return SbiRet::invalid_param();
        };
        let event = match Event::parse(event_idx, event_data) {
            Ok(event) => event,
            Err(error) => return error,
        };
        let selection = match CounterSelection::new(
            counter_idx_base,
            counter_idx_mask,
            self.total_counters(),
        ) {
            Ok(selection) => selection,
            Err(error) => return error,
        };
        let mut state = match self.current_state() {
            Ok(state) => state,
            Err(error) => return error,
        };

        if flags.contains(flags::CounterCfgFlags::SKIP_MATCH) {
            let Some(index) = selection.first() else {
                machine::abort(|| {});
            };
            return match self.configure_existing(index, event, flags, &mut state) {
                Ok(()) => SbiRet::success(index),
                Err(error) => error,
            };
        }

        match event.kind {
            EventKind::Firmware(event_code) => {
                for index in selection {
                    if index >= self.counters.count() && state.assignments[index].is_none() {
                        let firmware_index = index - self.counters.count();
                        if flags.contains(flags::CounterCfgFlags::CLEAR_VALUE) {
                            state.firmware_values[firmware_index] = 0;
                        }
                        state.assignments[index] = Some(Assignment::Firmware {
                            event_code,
                            running: flags.contains(flags::CounterCfgFlags::AUTO_START),
                        });
                        return SbiRet::success(index);
                    }
                }
            }
            EventKind::Hardware => {
                for index in selection {
                    if index >= self.counters.count() {
                        continue;
                    }
                    match self.configure_new_hardware(index, event, flags, &mut state) {
                        Ok(()) => return SbiRet::success(index),
                        Err(error) if error == SbiRet::not_supported() => {}
                        Err(error) => return error,
                    }
                }
            }
        }
        SbiRet::not_supported()
    }

    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> SbiRet {
        let Some(flags) = flags::CounterStartFlags::from_bits(start_flags) else {
            return SbiRet::invalid_param();
        };
        if flags.contains(flags::CounterStartFlags::INIT_SNAPSHOT) {
            return SbiRet::no_shmem();
        }
        let selection = match CounterSelection::new(
            counter_idx_base,
            counter_idx_mask,
            self.total_counters(),
        ) {
            Ok(selection) => selection,
            Err(error) => return error,
        };
        let mut state = match self.current_state() {
            Ok(state) => state,
            Err(error) => return error,
        };
        for index in selection {
            match state.assignments[index] {
                None => return SbiRet::invalid_param(),
                Some(Assignment::Hardware { running: true, .. })
                | Some(Assignment::Firmware { running: true, .. }) => {
                    return SbiRet::already_started();
                }
                Some(_) => {}
            }
        }

        let mut values = [None; COUNTER_LIMIT];
        for index in selection {
            values[index] = match state.assignments[index] {
                Some(Assignment::Hardware { counter, .. }) => match self.counters.read(counter) {
                    Ok(value) => Some(value),
                    Err(error) => return counter_error(error),
                },
                Some(Assignment::Firmware { .. }) => {
                    Some(state.firmware_values[index - self.counters.count()])
                }
                None => machine::abort(|| {}),
            };
        }

        for (completed, index) in selection.into_iter().enumerate() {
            let Some(assignment) = state.assignments[index] else {
                machine::abort(|| {});
            };
            match assignment {
                Assignment::Hardware {
                    counter,
                    event,
                    running: false,
                } => {
                    let initial = flags
                        .contains(flags::CounterStartFlags::INIT_VALUE)
                        .then_some(initial_value);
                    if let Err(error) = self.counters.start(counter, initial) {
                        self.rollback_started(&mut state, selection, completed, &values);
                        return counter_error(error);
                    }
                    state.assignments[index] = Some(Assignment::Hardware {
                        counter,
                        event,
                        running: true,
                    });
                }
                Assignment::Firmware {
                    event_code,
                    running: false,
                } => {
                    if flags.contains(flags::CounterStartFlags::INIT_VALUE) {
                        state.firmware_values[index - self.counters.count()] = initial_value;
                    }
                    state.assignments[index] = Some(Assignment::Firmware {
                        event_code,
                        running: true,
                    });
                }
                Assignment::Hardware { running: true, .. }
                | Assignment::Firmware { running: true, .. } => machine::abort(|| {}),
            }
        }
        SbiRet::success(0)
    }

    fn counter_stop(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        stop_flags: usize,
    ) -> SbiRet {
        let Some(flags) = flags::CounterStopFlags::from_bits(stop_flags) else {
            return SbiRet::invalid_param();
        };
        if flags.contains(flags::CounterStopFlags::TAKE_SNAPSHOT) {
            return SbiRet::no_shmem();
        }
        let selection = match CounterSelection::new(
            counter_idx_base,
            counter_idx_mask,
            self.total_counters(),
        ) {
            Ok(selection) => selection,
            Err(error) => return error,
        };
        let mut state = match self.current_state() {
            Ok(state) => state,
            Err(error) => return error,
        };
        for index in selection {
            match state.assignments[index] {
                None => return SbiRet::invalid_param(),
                Some(Assignment::Hardware { running: false, .. })
                | Some(Assignment::Firmware { running: false, .. }) => {
                    return SbiRet::already_stopped();
                }
                Some(_) => {}
            }
        }

        let mut values = [None; COUNTER_LIMIT];
        for index in selection {
            if let Some(Assignment::Hardware { counter, .. }) = state.assignments[index] {
                values[index] = match self.counters.read(counter) {
                    Ok(value) => Some(value),
                    Err(error) => return counter_error(error),
                };
            }
        }

        for (completed, index) in selection.into_iter().enumerate() {
            let Some(assignment) = state.assignments[index] else {
                machine::abort(|| {});
            };
            match assignment {
                Assignment::Hardware {
                    counter,
                    event,
                    running: true,
                } => {
                    if let Err(error) = self.counters.stop(counter) {
                        self.rollback_stopped(
                            &mut state,
                            selection,
                            completed,
                            &values,
                            flags.contains(flags::CounterStopFlags::RESET),
                        );
                        return counter_error(error);
                    }
                    if flags.contains(flags::CounterStopFlags::RESET)
                        && let Err(error) = self.counters.reset(counter)
                    {
                        if self.counters.start(counter, None).is_err() {
                            machine::abort(|| {});
                        }
                        self.rollback_stopped(&mut state, selection, completed, &values, true);
                        return counter_error(error);
                    }
                    state.assignments[index] = Some(Assignment::Hardware {
                        counter,
                        event,
                        running: false,
                    });
                }
                Assignment::Firmware {
                    event_code,
                    running: true,
                } => {
                    state.assignments[index] = Some(Assignment::Firmware {
                        event_code,
                        running: false,
                    });
                }
                Assignment::Hardware { running: false, .. }
                | Assignment::Firmware { running: false, .. } => machine::abort(|| {}),
            }
        }
        if flags.contains(flags::CounterStopFlags::RESET) {
            for index in selection {
                state.assignments[index] = None;
            }
        }
        SbiRet::success(0)
    }

    fn counter_fw_read(&self, counter_idx: usize) -> SbiRet {
        let hardware_count = self.counters.count();
        if !(hardware_count..self.total_counters()).contains(&counter_idx) {
            return SbiRet::invalid_param();
        }
        let state = match self.current_state() {
            Ok(state) => state,
            Err(error) => return error,
        };
        if !matches!(
            state.assignments[counter_idx],
            Some(Assignment::Firmware { .. })
        ) {
            return SbiRet::invalid_param();
        }
        SbiRet::success(state.firmware_values[counter_idx - hardware_count] as usize)
    }

    fn counter_fw_read_hi(&self, counter_idx: usize) -> SbiRet {
        #[cfg(target_pointer_width = "64")]
        {
            let _ = counter_idx;
            SbiRet::success(0)
        }
        #[cfg(target_pointer_width = "32")]
        {
            let hardware_count = self.counters.count();
            if !(hardware_count..self.total_counters()).contains(&counter_idx) {
                return SbiRet::invalid_param();
            }
            let state = match self.current_state() {
                Ok(state) => state,
                Err(error) => return error,
            };
            if !matches!(
                state.assignments[counter_idx],
                Some(Assignment::Firmware { .. })
            ) {
                return SbiRet::invalid_param();
            }
            SbiRet::success((state.firmware_values[counter_idx - hardware_count] >> 32) as usize)
        }
    }
}

impl Event {
    fn parse(index: usize, data: u64) -> Result<Self, SbiRet> {
        if index & !EVENT_INDEX_MASK != 0 {
            return Err(SbiRet::invalid_param());
        }
        let kind = (index >> 16) & 0xf;
        let code = index & 0xffff;
        match kind {
            event_type::HARDWARE_GENERAL
                if (hardware_event::CPU_CYCLES..=hardware_event::REF_CPU_CYCLES)
                    .contains(&code) =>
            {
                Ok(Self {
                    index,
                    selector: index as u64,
                    kind: EventKind::Hardware,
                })
            }
            event_type::HARDWARE_CACHE if valid_cache_event(code) => Ok(Self {
                index,
                selector: index as u64,
                kind: EventKind::Hardware,
            }),
            event_type::HARDWARE_RAW | event_type::HARDWARE_RAW_V2 if code == 0 => Ok(Self {
                index,
                selector: data,
                kind: EventKind::Hardware,
            }),
            event_type::FIRMWARE if supported_firmware_event(code) && data == 0 => Ok(Self {
                index,
                selector: 0,
                kind: EventKind::Firmware(code),
            }),
            event_type::FIRMWARE
                if code <= firmware_event::HFENCE_VVMA_ASID_RECEIVED
                    || code == firmware_event::PLATFORM =>
            {
                Err(SbiRet::not_supported())
            }
            event_type::HARDWARE_GENERAL
            | event_type::HARDWARE_CACHE
            | event_type::HARDWARE_RAW
            | event_type::HARDWARE_RAW_V2
            | event_type::FIRMWARE => Err(SbiRet::invalid_param()),
            _ => Err(SbiRet::invalid_param()),
        }
    }
}

fn valid_cache_event(code: usize) -> bool {
    let cache = (code >> 3) & 0x1fff;
    let operation = (code >> 1) & 0x3;
    let result = code & 1;
    cache <= cache_event::NODE
        && operation <= cache_operation::PREFETCH
        && result <= cache_result::MISS
}

fn supported_firmware_event(code: usize) -> bool {
    let base = matches!(
        code,
        firmware_event::MISALIGNED_LOAD
            | firmware_event::MISALIGNED_STORE
            | firmware_event::ILLEGAL_INSN
            | firmware_event::SET_TIMER
            | firmware_event::IPI_SENT
            | firmware_event::FENCE_I_SENT
            | firmware_event::SFENCE_VMA_SENT
            | firmware_event::SFENCE_VMA_ASID_SENT
    );
    #[cfg(feature = "hypervisor")]
    {
        base || matches!(
            code,
            firmware_event::HFENCE_GVMA_SENT
                | firmware_event::HFENCE_GVMA_VMID_SENT
                | firmware_event::HFENCE_VVMA_SENT
                | firmware_event::HFENCE_VVMA_ASID_SENT
        )
    }
    #[cfg(not(feature = "hypervisor"))]
    base
}

fn apply_hardware_config(
    counters: &PerformanceCounters,
    counter: CounterId,
    flags: flags::CounterCfgFlags,
) -> Result<bool, SbiRet> {
    let clear = flags.contains(flags::CounterCfgFlags::CLEAR_VALUE);
    let auto_start = flags.contains(flags::CounterCfgFlags::AUTO_START);
    if clear || auto_start {
        counters
            .start(counter, clear.then_some(0))
            .map_err(counter_error)?;
        if !auto_start && let Err(error) = counters.stop(counter) {
            let _ = counters.reset(counter);
            return Err(counter_error(error));
        }
    }
    Ok(auto_start)
}

fn counter_error(error: CounterError) -> SbiRet {
    match error {
        CounterError::InvalidCounter => SbiRet::invalid_param(),
        CounterError::UnsupportedEvent => SbiRet::not_supported(),
        CounterError::AlreadyStarted => SbiRet::already_started(),
        CounterError::AlreadyStopped => SbiRet::already_stopped(),
        CounterError::MechanismFailure => SbiRet::failed(),
    }
}

#[derive(Clone, Copy)]
struct CounterSelection {
    base: usize,
    mask: usize,
}

impl CounterSelection {
    fn new(base: usize, mask: usize, total: usize) -> Result<Self, SbiRet> {
        if mask == 0 || base >= total {
            return Err(SbiRet::invalid_param());
        }
        let highest = usize::BITS as usize - 1 - mask.leading_zeros() as usize;
        if base.checked_add(highest).is_none_or(|index| index >= total) {
            return Err(SbiRet::invalid_param());
        }
        Ok(Self { base, mask })
    }

    fn first(self) -> Option<usize> {
        self.into_iter().next()
    }

    fn take(self, count: usize) -> impl Iterator<Item = usize> {
        self.into_iter().take(count)
    }
}

impl IntoIterator for CounterSelection {
    type Item = usize;
    type IntoIter = CounterSelectionIter;

    fn into_iter(self) -> Self::IntoIter {
        CounterSelectionIter(self)
    }
}

struct CounterSelectionIter(CounterSelection);

impl Iterator for CounterSelectionIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let bit = self.0.mask.trailing_zeros();
        if bit == usize::BITS {
            return None;
        }
        self.0.mask &= !(1usize << bit);
        Some(self.0.base + bit as usize)
    }
}
