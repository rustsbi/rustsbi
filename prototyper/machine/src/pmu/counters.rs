//! Public performance-counter capability operations.

use alloc::sync::Arc;
use alloc::vec;
use core::ops::Deref;

use crate::config::HART_CAPACITY;
use crate::hart::{HartLocal, HartLocalError};

use super::control::*;
use super::hart::*;
use super::probe::probe_current;

/// Safe access to hardware performance counters on every admitted hart.
pub struct PerformanceCounters {
    facts: Arc<HartLocal<HartCounters>>,
}

impl PerformanceCounters {
    pub(crate) fn unprepared() -> Result<Self, CounterError> {
        let facts = vec![HartCounters::UNINITIALIZED; HART_CAPACITY];
        Ok(Self {
            facts: Arc::new(HartLocal::new(facts).map_err(map_local_error)?),
        })
    }

    pub(crate) fn share(&self) -> Self {
        Self {
            facts: Arc::clone(&self.facts),
        }
    }

    /// Probes and normalizes the calling hart before it becomes runnable.
    pub(crate) fn prepare_current(&self) -> Result<(), CounterError> {
        let facts = probe_current()?;
        reset_all(facts)?;
        *self.facts.current().map_err(map_local_error)? = facts;
        Ok(())
    }

    /// Returns the number of counters visible on the calling hart.
    pub fn count(&self) -> usize {
        self.current_facts().map_or(0, |facts| facts.count())
    }

    /// Returns read-only facts for one dense current-hart counter index.
    pub fn info(&self, index: usize) -> Result<CounterInfo, CounterError> {
        let facts = self.current_facts()?;
        let offset = facts.validate(index)?;
        Ok(CounterInfo {
            csr_number: supervisor_csr(offset),
            width: 64,
        })
    }

    /// Selects an event while leaving the counter stopped.
    ///
    /// Upper policy validates SBI event mappings. For a programmable counter,
    /// `event_data` is the already selected architectural event value.
    pub fn configure(
        &self,
        index: usize,
        event_id: usize,
        event_data: u64,
    ) -> Result<(), CounterError> {
        if event_id & !EVENT_INDEX_MASK != 0 {
            return Err(CounterError::UnsupportedEvent);
        }
        let facts = self.current_facts()?;
        let offset = facts.validate(index)?;
        match offset {
            CYCLE_OFFSET if event_id == CYCLE_EVENT => Ok(()),
            INSTRET_OFFSET if event_id == INSTRUCTION_EVENT => Ok(()),
            CYCLE_OFFSET | INSTRET_OFFSET => Err(CounterError::UnsupportedEvent),
            FIRST_PROGRAMMABLE_OFFSET..=LAST_COUNTER_OFFSET => {
                require_stopped(offset)?;
                let wide = facts.event_is_wide(offset);
                let previous = read_event(offset, wide)?;
                match write_event(offset, event_data, wide) {
                    Ok(()) => Ok(()),
                    Err(error) => {
                        restore_or_abort(write_event(offset, previous, wide));
                        Err(error)
                    }
                }
            }
            _ => Err(CounterError::InvalidCounter),
        }
    }

    /// Starts a stopped counter, optionally replacing its initial value.
    pub fn start(&self, index: usize, initial: Option<u64>) -> Result<(), CounterError> {
        let facts = self.current_facts()?;
        let offset = facts.validate(index)?;
        require_stopped(offset)?;
        let previous = initial.map(|_| read_counter(offset)).transpose()?;
        if let Some(value) = initial
            && let Err(error) = write_counter(offset, value)
        {
            restore_or_abort(write_counter(offset, previous.unwrap_or(0)));
            return Err(error);
        }
        if let Err(error) = set_inhibited(offset, false) {
            restore_or_abort(set_inhibited(offset, true));
            if let Some(previous) = previous {
                restore_or_abort(write_counter(offset, previous));
            }
            return Err(error);
        }
        Ok(())
    }

    /// Stops a running counter without discarding its event assignment.
    pub fn stop(&self, index: usize) -> Result<(), CounterError> {
        let facts = self.current_facts()?;
        let offset = facts.validate(index)?;
        require_started(offset)?;
        match set_inhibited(offset, true) {
            Ok(()) => Ok(()),
            Err(error) => {
                restore_or_abort(set_inhibited(offset, false));
                Err(error)
            }
        }
    }

    /// Replaces the value of a stopped counter without changing its event.
    pub fn set_value(&self, index: usize, value: u64) -> Result<(), CounterError> {
        let facts = self.current_facts()?;
        let offset = facts.validate(index)?;
        require_stopped(offset)?;
        let previous = read_counter(offset)?;
        match write_counter(offset, value) {
            Ok(()) => Ok(()),
            Err(error) => {
                restore_or_abort(write_counter(offset, previous));
                Err(error)
            }
        }
    }

    /// Resets a stopped counter to its unassigned baseline.
    pub fn reset(&self, index: usize) -> Result<(), CounterError> {
        let facts = self.current_facts()?;
        let offset = facts.validate(index)?;
        require_stopped(offset)?;
        let previous_value = read_counter(offset)?;
        let wide = facts.event_is_wide(offset);
        let previous_event = (offset >= FIRST_PROGRAMMABLE_OFFSET)
            .then(|| read_event(offset, wide))
            .transpose()?;
        let operation = write_counter(offset, 0).and_then(|()| {
            if offset >= FIRST_PROGRAMMABLE_OFFSET {
                write_event(offset, 0, wide)
            } else {
                Ok(())
            }
        });
        if let Err(error) = operation {
            if let Some(previous_event) = previous_event {
                restore_or_abort(write_event(offset, previous_event, wide));
            }
            restore_or_abort(write_counter(offset, previous_value));
            return Err(error);
        }
        Ok(())
    }

    /// Reads one complete 64-bit counter value.
    pub fn read(&self, index: usize) -> Result<u64, CounterError> {
        let facts = self.current_facts()?;
        let offset = facts.validate(index)?;
        read_counter(offset)
    }

    /// Restores all programmable counters to the cold-start baseline.
    pub fn reset_all(&self) -> Result<(), CounterError> {
        let facts = self.current_facts()?;
        reset_all(*facts)
    }

    fn current_facts(&self) -> Result<impl Deref<Target = HartCounters> + '_, CounterError> {
        self.facts.current().map_err(map_local_error)
    }

    pub(crate) fn accessible_mask(&self) -> Result<u32, CounterError> {
        Ok(self.current_facts()?.accessible)
    }

    pub(crate) fn retain_current(&self, accessible: u32) -> Result<(), CounterError> {
        let mut facts = self.facts.current().map_err(map_local_error)?;
        facts.accessible &= accessible;
        facts.controllable &= accessible;
        Ok(())
    }
}

fn map_local_error(_: HartLocalError) -> CounterError {
    CounterError::MechanismFailure
}
