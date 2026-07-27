//! Per-hart SBI PMU assignment state.

use machine::{CounterError, PerformanceCounters};
use sbi_spec::pmu::hardware_event;

use super::event::{Event, EventKind};
use super::{COUNTER_LIMIT, FIRMWARE_COUNTER_COUNT};

#[derive(Clone, Copy)]
pub(super) struct HartState {
    pub(super) initialized: bool,
    pub(super) assignments: [Option<Assignment>; COUNTER_LIMIT],
    pub(super) firmware_values: [u64; FIRMWARE_COUNTER_COUNT],
}

#[derive(Clone, Copy)]
pub(super) enum Assignment {
    Hardware {
        counter: usize,
        event: Event,
        running: bool,
    },
    Firmware {
        event_code: usize,
        running: bool,
    },
}

impl HartState {
    pub(super) const NEW: Self = Self {
        initialized: false,
        assignments: [None; COUNTER_LIMIT],
        firmware_values: [0; FIRMWARE_COUNTER_COUNT],
    };

    pub(super) fn initialize_fixed(
        &mut self,
        counters: &PerformanceCounters,
    ) -> Result<(), CounterError> {
        if self.initialized {
            return Ok(());
        }
        for index in 0..counters.count() {
            let info = counters.info(index)?;
            let event_index = match info.csr_number() {
                0x0c00 => Some(hardware_event::CPU_CYCLES),
                0x0c02 => Some(hardware_event::INSTRUCTIONS),
                _ => None,
            };
            if let Some(event_index) = event_index {
                self.assignments[index] = Some(Assignment::Hardware {
                    counter: index,
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
