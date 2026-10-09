//! Per-hart PMU event assignments and firmware counter state.

use runtime::pmu::{
    CounterError, CounterEventUpdate, CounterStartMode, CounterValueUpdate, HardwareCounter, Pmu,
};
use sbi_spec::pmu::{flags, hardware_event};

use crate::sbi::hart_local::with_current;

use super::event::EventIdx;

/// Maximum number of hardware performance counters supported.
const PMU_HARDWARE_COUNTER_MAX: usize = 32;
/// Maximum number of firmware-managed counters supported.
const PMU_FIRMWARE_COUNTER_MAX: usize = 16;
/// Marker value for inactive/invalid event indices.
pub(super) const PMU_EVENT_IDX_INVALID: usize = usize::MAX;

/// Per-hart event assignments and firmware counter values.
/// Hardware values and their running state are managed by Runtime.
pub(crate) struct PmuState {
    pub(super) active_event: [usize; PMU_HARDWARE_COUNTER_MAX + PMU_FIRMWARE_COUNTER_MAX],
    /// Bitmap of running firmware counters (one bit per counter).
    fw_counter_state: usize,
    /// Firmware counter values.
    fw_counter: [u64; PMU_FIRMWARE_COUNTER_MAX],
    pub(super) hw_counters_num: usize,
    /// Architectural positions corresponding to the dense SBI hardware index space.
    hardware_positions: u32,
}

impl PmuState {
    /// Builds the SBI index layout from architectural counter positions.
    pub(crate) fn new(mhpm_mask: u32) -> Self {
        let hw_counters_num = mhpm_mask.count_ones() as usize;

        let mut active_event =
            [PMU_EVENT_IDX_INVALID; PMU_HARDWARE_COUNTER_MAX + PMU_FIRMWARE_COUNTER_MAX];

        active_event[0] = hardware_event::CPU_CYCLES;
        active_event[2] = hardware_event::INSTRUCTIONS;

        Self {
            active_event,
            fw_counter_state: 0,
            fw_counter: [0; PMU_FIRMWARE_COUNTER_MAX],
            hw_counters_num,
            hardware_positions: mhpm_mask,
        }
    }

    /// Returns the number of SBI hardware index slots, including the time hole.
    pub(super) fn hardware_counter_count(&self) -> usize {
        self.hw_counters_num
    }

    /// Maps a dense SBI hardware index to its actual architectural position.
    fn hardware_position(&self, counter_idx: usize) -> Result<usize, CounterError> {
        if counter_idx >= self.hw_counters_num {
            return Err(CounterError::InvalidCounter);
        }
        let mut remaining = self.hardware_positions;
        for _ in 0..counter_idx {
            remaining &= remaining - 1;
        }
        Ok(remaining.trailing_zeros() as usize)
    }

    /// Resolves the SBI-selected counter through the current hart's PMU capability.
    pub(super) fn hardware_counter(
        &self,
        counter_idx: usize,
    ) -> Result<HardwareCounter, CounterError> {
        let position = self.hardware_position(counter_idx)?;
        Pmu::current()?.counter(position)
    }

    /// Returns the total number of counters (hardware + firmware).
    pub(super) fn counter_count(&self) -> usize {
        self.hw_counters_num + PMU_FIRMWARE_COUNTER_MAX
    }

    /// Reads a configured firmware counter after validating its SBI index and event.
    pub(super) fn firmware_counter_value(&self, counter_idx: usize) -> Option<u64> {
        if counter_idx < self.hw_counters_num || counter_idx >= self.counter_count() {
            return None;
        }
        let event = self.active_event[counter_idx];
        if event == PMU_EVENT_IDX_INVALID || !EventIdx::new(event).firmware_event_valid() {
            return None;
        }
        let fw_idx = counter_idx - self.hw_counters_num;

        Some(self.fw_counter[fw_idx])
    }

    /// Starts a firmware counter, optionally replacing its value.
    pub(super) fn start_firmware_counter(
        &mut self,
        counter_idx: usize,
        initial_value: Option<u64>,
    ) -> Result<(), CounterError> {
        if counter_idx < self.hw_counters_num || counter_idx >= self.counter_count() {
            return Err(CounterError::InvalidCounter);
        }
        let fw_idx = counter_idx - self.hw_counters_num;

        if self.fw_counter_state & (1 << fw_idx) != 0 {
            return Err(CounterError::AlreadyStarted);
        }

        if let Some(value) = initial_value {
            self.fw_counter[fw_idx] = value;
        }
        self.fw_counter_state |= 1 << fw_idx;
        Ok(())
    }

    /// Stops a firmware counter and optionally releases its event assignment.
    pub(super) fn stop_firmware_counter(
        &mut self,
        counter_idx: usize,
        event_update: CounterEventUpdate,
    ) -> Result<(), CounterError> {
        if counter_idx < self.hw_counters_num || counter_idx >= self.counter_count() {
            return Err(CounterError::InvalidCounter);
        }
        let fw_idx = counter_idx - self.hw_counters_num;

        if self.fw_counter_state & (1 << fw_idx) == 0 {
            return Err(CounterError::AlreadyStopped);
        }

        if event_update == CounterEventUpdate::Clear {
            self.active_event[counter_idx] = PMU_EVENT_IDX_INVALID;
        }
        self.fw_counter_state &= !(1 << fw_idx);
        Ok(())
    }

    /// Tests the running state of a firmware counter identified by its SBI index.
    pub(super) fn firmware_counter_running(&self, counter_idx: usize) -> bool {
        if counter_idx < self.hw_counters_num || counter_idx >= self.counter_count() {
            return false;
        }
        let fw_idx = counter_idx - self.hw_counters_num;
        self.fw_counter_state & (1 << fw_idx) != 0
    }

    /// Configures a stopped counter without publishing its new event assignment.
    pub(super) fn configure_counter(
        &mut self,
        counter_idx: usize,
        event: EventIdx,
        selector: Option<u64>,
        flags: flags::ConfigFlags,
    ) -> Result<(), CounterError> {
        let auto_start = flags.contains(flags::ConfigFlags::AUTO_START);
        let clear_value = flags.contains(flags::ConfigFlags::CLEAR_VALUE);
        if event.is_firmware_event() {
            let firmware_index = counter_idx
                .checked_sub(self.hw_counters_num)
                .ok_or(CounterError::InvalidCounter)?;
            if firmware_index >= PMU_FIRMWARE_COUNTER_MAX {
                return Err(CounterError::InvalidCounter);
            }
            if self.firmware_counter_running(counter_idx) {
                return Err(CounterError::AlreadyStarted);
            }
            if clear_value {
                self.fw_counter[firmware_index] = 0;
            }
            if auto_start {
                self.fw_counter_state |= 1 << firmware_index;
            }
        } else {
            let value_update = if clear_value {
                CounterValueUpdate::Clear
            } else {
                CounterValueUpdate::Preserve
            };
            let start_mode = if auto_start {
                CounterStartMode::Start
            } else {
                CounterStartMode::Preserve
            };
            self.hardware_counter(counter_idx)
                .and_then(|counter| counter.configure(selector, value_update, start_mode))?;
        }
        Ok(())
    }
}

pub fn pmu_firmware_counter_increment(firmware_event: usize) {
    if crate::sbi::pmu().is_none() {
        return;
    }
    with_current(|local| {
        local.with_pmu(|pmu_state| {
            // Avoid scanning firmware counters when none are running.
            if pmu_state.fw_counter_state == 0 {
                return;
            }
            let counter_idx_start = pmu_state.hw_counters_num;
            for counter_idx in counter_idx_start..counter_idx_start + PMU_FIRMWARE_COUNTER_MAX {
                let fw_idx = counter_idx - counter_idx_start;
                if pmu_state.active_event[counter_idx]
                    == EventIdx::from_firmware_event(firmware_event).raw()
                    && pmu_state.firmware_counter_running(counter_idx)
                {
                    pmu_state.fw_counter[fw_idx] = pmu_state.fw_counter[fw_idx].wrapping_add(1);
                }
            }
        });
    });
}
