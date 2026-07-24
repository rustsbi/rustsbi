//! Performance-monitoring SBI policy.

use alloc::vec;

use machine::{CounterError, HartLocal, HartLocalError, PerformanceCounters};
use rustsbi::SbiRet;
use sbi_spec::{binary::Error, pmu::flags};

mod event;
mod selection;
mod state;

use event::{Event, EventKind};
use selection::CounterSelection;
use state::{Assignment, HartState};

const HARDWARE_COUNTER_LIMIT: usize = 32;
const FIRMWARE_COUNTER_COUNT: usize = 16;
const COUNTER_LIMIT: usize = HARDWARE_COUNTER_LIMIT + FIRMWARE_COUNTER_COUNT;

/// Upper SBI service that assigns architectural and firmware events to the
/// calling hart's counters.
pub(crate) struct PerformanceMonitor {
    counters: PerformanceCounters,
    state: HartLocal<HartState>,
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

    fn current_state(&self) -> Result<machine::HartLocalGuard<'_, HartState>, Error> {
        let mut state = self.state.current().map_err(|_| Error::Failed)?;
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
    ) -> Result<(), Error> {
        if index >= self.counters.count() {
            return Err(Error::InvalidParam);
        }
        let counter = index;
        if state.assignments[index].is_some() {
            return Err(Error::NotSupported);
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
    ) -> Result<(), Error> {
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
            _ => Err(Error::InvalidParam),
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
            return match self.counters.info(counter_idx) {
                Ok(info) => SbiRet::success(
                    usize::from(info.csr_number()) | (usize::from(info.width() - 1) << 12),
                ),
                Err(error) => counter_error(error).into(),
            };
        }
        if counter_idx < self.total_counters() {
            SbiRet::success(1usize << (usize::BITS - 1))
        } else {
            Error::InvalidParam.into()
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
            return Error::InvalidParam.into();
        };
        let event = match Event::parse(event_idx, event_data) {
            Ok(event) => event,
            Err(error) => return error.into(),
        };
        let selection = match CounterSelection::new(
            counter_idx_base,
            counter_idx_mask,
            self.total_counters(),
        ) {
            Ok(selection) => selection,
            Err(error) => return error.into(),
        };
        let mut state = match self.current_state() {
            Ok(state) => state,
            Err(error) => return error.into(),
        };

        if flags.contains(flags::CounterCfgFlags::SKIP_MATCH) {
            let Some(index) = selection.first() else {
                machine::abort(|| {});
            };
            return match self.configure_existing(index, event, flags, &mut state) {
                Ok(()) => SbiRet::success(index),
                Err(error) => error.into(),
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
                        Err(Error::NotSupported) => {}
                        Err(error) => return error.into(),
                    }
                }
            }
        }
        Error::NotSupported.into()
    }

    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> SbiRet {
        let Some(flags) = flags::CounterStartFlags::from_bits(start_flags) else {
            return Error::InvalidParam.into();
        };
        if flags.contains(flags::CounterStartFlags::INIT_SNAPSHOT) {
            return Error::NoShmem.into();
        }
        let selection = match CounterSelection::new(
            counter_idx_base,
            counter_idx_mask,
            self.total_counters(),
        ) {
            Ok(selection) => selection,
            Err(error) => return error.into(),
        };
        let mut state = match self.current_state() {
            Ok(state) => state,
            Err(error) => return error.into(),
        };
        for index in selection {
            match state.assignments[index] {
                None => return Error::InvalidParam.into(),
                Some(Assignment::Hardware { running: true, .. })
                | Some(Assignment::Firmware { running: true, .. }) => {
                    return Error::AlreadyStarted.into();
                }
                Some(_) => {}
            }
        }

        let mut values = [None; COUNTER_LIMIT];
        for index in selection {
            values[index] = match state.assignments[index] {
                Some(Assignment::Hardware { counter, .. }) => match self.counters.read(counter) {
                    Ok(value) => Some(value),
                    Err(error) => return counter_error(error).into(),
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
                        return counter_error(error).into();
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
            return Error::InvalidParam.into();
        };
        if flags.contains(flags::CounterStopFlags::TAKE_SNAPSHOT) {
            return Error::NoShmem.into();
        }
        let selection = match CounterSelection::new(
            counter_idx_base,
            counter_idx_mask,
            self.total_counters(),
        ) {
            Ok(selection) => selection,
            Err(error) => return error.into(),
        };
        let mut state = match self.current_state() {
            Ok(state) => state,
            Err(error) => return error.into(),
        };
        for index in selection {
            match state.assignments[index] {
                None => return Error::InvalidParam.into(),
                Some(Assignment::Hardware { running: false, .. })
                | Some(Assignment::Firmware { running: false, .. }) => {
                    return Error::AlreadyStopped.into();
                }
                Some(_) => {}
            }
        }

        let mut values = [None; COUNTER_LIMIT];
        for index in selection {
            if let Some(Assignment::Hardware { counter, .. }) = state.assignments[index] {
                values[index] = match self.counters.read(counter) {
                    Ok(value) => Some(value),
                    Err(error) => return counter_error(error).into(),
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
                        return counter_error(error).into();
                    }
                    if flags.contains(flags::CounterStopFlags::RESET)
                        && let Err(error) = self.counters.reset(counter)
                    {
                        if self.counters.start(counter, None).is_err() {
                            machine::abort(|| {});
                        }
                        self.rollback_stopped(&mut state, selection, completed, &values, true);
                        return counter_error(error).into();
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
            return Error::InvalidParam.into();
        }
        let state = match self.current_state() {
            Ok(state) => state,
            Err(error) => return error.into(),
        };
        if !matches!(
            state.assignments[counter_idx],
            Some(Assignment::Firmware { .. })
        ) {
            return Error::InvalidParam.into();
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
                return Error::InvalidParam.into();
            }
            let state = match self.current_state() {
                Ok(state) => state,
                Err(error) => return error.into(),
            };
            if !matches!(
                state.assignments[counter_idx],
                Some(Assignment::Firmware { .. })
            ) {
                return Error::InvalidParam.into();
            }
            SbiRet::success((state.firmware_values[counter_idx - hardware_count] >> 32) as usize)
        }
    }
}

fn apply_hardware_config(
    counters: &PerformanceCounters,
    counter: usize,
    flags: flags::CounterCfgFlags,
) -> Result<bool, Error> {
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

fn counter_error(error: CounterError) -> Error {
    match error {
        CounterError::InvalidCounter => Error::InvalidParam,
        CounterError::UnsupportedEvent => Error::NotSupported,
        CounterError::AlreadyStarted => Error::AlreadyStarted,
        CounterError::AlreadyStopped => Error::AlreadyStopped,
        CounterError::MechanismFailure => Error::Failed,
    }
}
