//! Performance monitoring.
//!
//! # References
//!
//! - Specification: [RISC-V SBI PMU extension](https://docs.riscv.org/reference/sbi/v3.0/ext-pmu.html) —
//!   counter discovery, event configuration, and shared-memory operations.

use runtime::pmu::{CounterError, CounterEventUpdate};
use runtime::rustsbi::{Pmu, SbiRet};
use sbi_spec::binary::SharedPtr;
use sbi_spec::pmu::shmem_size::SIZE;
use sbi_spec::pmu::*;

use super::hart_local::with_current;

mod counter_mask;
mod event;
mod state;

use counter_mask::CounterMask;
use event::{EventError, EventIdx};
pub(crate) use event::{SbiPmu, from_node};
use state::PMU_EVENT_IDX_INVALID;
pub(crate) use state::{PmuState, pmu_firmware_counter_increment};

/// Runtime firmware-event sink for SBI PMU counters.
struct RuntimeTrapCounters;

static RUNTIME_TRAP_COUNTERS: RuntimeTrapCounters = RuntimeTrapCounters;

impl runtime::events::TrapCounters for RuntimeTrapCounters {
    fn record_access_load(&self) {
        pmu_firmware_counter_increment(firmware_event::ACCESS_LOAD);
    }

    fn record_access_store(&self) {
        pmu_firmware_counter_increment(firmware_event::ACCESS_STORE);
    }

    fn record_illegal_instruction(&self) {
        pmu_firmware_counter_increment(firmware_event::ILLEGAL_INSN);
    }

    fn record_misaligned_load(&self) {
        pmu_firmware_counter_increment(firmware_event::MISALIGNED_LOAD);
    }

    fn record_misaligned_store(&self) {
        pmu_firmware_counter_increment(firmware_event::MISALIGNED_STORE);
    }
}

/// Returns the Runtime firmware-event sink adapter.
pub(crate) fn runtime_counters() -> &'static dyn runtime::events::TrapCounters {
    &RUNTIME_TRAP_COUNTERS
}

impl Pmu for SbiPmu {
    /// Returns the number of hardware and firmware counters (FID #0).
    fn num_counters(&self) -> usize {
        with_current(|local| local.with_pmu(|state| state.counter_count()))
    }

    /// Returns the SBI counter type, CSR number, and width (FID #1).
    fn counter_get_info(&self, counter_idx: usize) -> SbiRet {
        with_current(|local| {
            local.with_pmu(|state| {
                if counter_idx >= state.counter_count() {
                    return SbiRet::invalid_param();
                }
                if counter_idx < state.hardware_counter_count() {
                    let counter = match state.hardware_counter(counter_idx) {
                        Ok(counter) => counter,
                        Err(error) => return hardware_error(error),
                    };
                    let width = match counter.width() {
                        Ok(width) => width,
                        Err(error) => return hardware_error(error),
                    };
                    return SbiRet::success(
                        CounterInfo::with_hardware_info(
                            counter.supervisor_csr_number(),
                            (width - 1) as u8,
                        )
                        .inner(),
                    );
                }

                SbiRet::success(CounterInfo::with_firmware_info().inner())
            })
        })
    }

    /// Finds and configures a counter from the requested selection (FID #2).
    fn counter_config_matching(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        config_flags: usize,
        event_idx: usize,
        event_data: u64,
    ) -> SbiRet {
        let flags = match flags::ConfigFlags::from_bits(config_flags) {
            Some(flags) => flags,
            None => return SbiRet::invalid_param(),
        };

        let event = EventIdx::new(event_idx);
        let is_firmware_event = event.is_firmware_event();

        with_current(|local| {
            local.with_pmu(|pmu_state| {
                let mut counters = match CounterMask::new(
                    counter_idx_base,
                    counter_idx_mask,
                    pmu_state.counter_count(),
                ) {
                    Ok(counters) => counters,
                    Err(error) => return hardware_error(error),
                };
                if !event.check_event_type() || (is_firmware_event && !event.firmware_event_valid())
                {
                    return SbiRet::invalid_param();
                }

                let skip_match = flags.contains(flags::ConfigFlags::SKIP_MATCH);

                let counter_idx;

                if skip_match {
                    // `SKIP_MATCH` reuses the first selected counter's existing assignment.
                    if let Some(ctr_idx) = counters.next() {
                        if pmu_state.active_event[ctr_idx] == PMU_EVENT_IDX_INVALID {
                            return SbiRet::invalid_param();
                        }
                        counter_idx = ctr_idx;
                    } else {
                        return SbiRet::invalid_param();
                    }
                } else {
                    let match_result = if event.is_firmware_event() {
                        self.find_firmware_counter(counters, pmu_state)
                    } else {
                        self.find_hardware_counter(counters, event_idx, event_data, pmu_state)
                    };
                    match match_result {
                        Ok(ctr_idx) => {
                            counter_idx = ctr_idx;
                        }
                        Err(err) => {
                            return event_error(err);
                        }
                    }
                }

                let selector = if is_firmware_event || skip_match {
                    None
                } else {
                    let counter = match pmu_state.hardware_counter(counter_idx) {
                        Ok(counter) => counter,
                        Err(error) => return hardware_error(error),
                    };
                    match self.hardware_event_selector(counter, event_idx, event_data) {
                        Ok(selector) => selector,
                        Err(error) => return event_error(error),
                    }
                };
                match pmu_state.configure_counter(counter_idx, event, selector, flags) {
                    Ok(_) => {
                        if !skip_match {
                            pmu_state.active_event[counter_idx] = event_idx;
                        }
                        SbiRet::success(counter_idx)
                    }
                    // SBI FID #2 treats a running counter as unavailable.
                    Err(CounterError::AlreadyStarted) => SbiRet::not_supported(),
                    Err(error) => hardware_error(error),
                }
            })
        })
    }

    /// Starts the selected counters (FID #3).
    fn counter_start(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        start_flags: usize,
        initial_value: u64,
    ) -> SbiRet {
        let flags = match flags::StartFlags::from_bits(start_flags) {
            Some(flags) => flags,
            None => return SbiRet::invalid_param(),
        };

        with_current(|local| {
            local.with_pmu(|pmu_state| {
                let counters = match CounterMask::new(
                    counter_idx_base,
                    counter_idx_mask,
                    pmu_state.counter_count(),
                ) {
                    Ok(counters) => counters,
                    Err(error) => return hardware_error(error),
                };

                // Validate every mapping before starting any selected counter.
                // A previous RESET releases the event and requires configuration.
                for counter_idx in counters {
                    if pmu_state.active_event[counter_idx] == PMU_EVENT_IDX_INVALID {
                        return SbiRet::invalid_param();
                    }
                }
                if flags.contains(flags::StartFlags::INIT_SNAPSHOT) {
                    return SbiRet::no_shmem();
                }

                let initial_value = flags
                    .contains(flags::StartFlags::INIT_VALUE)
                    .then_some(initial_value);
                for counter_idx in counters {
                    let start_result = if counter_idx >= pmu_state.hardware_counter_count() {
                        pmu_state.start_firmware_counter(counter_idx, initial_value)
                    } else {
                        pmu_state
                            .hardware_counter(counter_idx)
                            .and_then(|counter| counter.start(initial_value))
                    };
                    match start_result {
                        Ok(_) => {}
                        Err(error) => return hardware_error(error),
                    }
                }
                SbiRet::success(0)
            })
        })
    }

    /// Stops the selected counters and optionally releases their events (FID #4).
    fn counter_stop(
        &self,
        counter_idx_base: usize,
        counter_idx_mask: usize,
        stop_flags: usize,
    ) -> SbiRet {
        let flags = match flags::StopFlags::from_bits(stop_flags) {
            Some(flags) => flags,
            None => return SbiRet::invalid_param(),
        };

        with_current(|local| {
            local.with_pmu(|pmu_state| {
                let is_reset = flags.contains(flags::StopFlags::RESET);
                let event_update = if is_reset {
                    CounterEventUpdate::Clear
                } else {
                    CounterEventUpdate::Preserve
                };

                let counters = match CounterMask::new(
                    counter_idx_base,
                    counter_idx_mask,
                    pmu_state.counter_count(),
                ) {
                    Ok(counters) => counters,
                    Err(error) => return hardware_error(error),
                };

                if flags.contains(flags::StopFlags::TAKE_SNAPSHOT) {
                    return SbiRet::no_shmem();
                }

                let mut result = SbiRet::invalid_param();
                for counter_idx in counters {
                    let stop_result = if counter_idx >= pmu_state.hardware_counter_count() {
                        pmu_state.stop_firmware_counter(counter_idx, event_update)
                    } else {
                        let counter = match pmu_state.hardware_counter(counter_idx) {
                            Ok(counter) => counter,
                            // Keep stopping the other selected counters across the time hole.
                            Err(CounterError::TimeCounter) => continue,
                            Err(error) => return hardware_error(error),
                        };
                        let result = counter.stop(event_update);
                        // RESET releases a configured counter even if it was never started.
                        // The selector is cleared before `AlreadyStopped` is reported.
                        if is_reset && matches!(result, Ok(()) | Err(CounterError::AlreadyStopped))
                        {
                            pmu_state.active_event[counter_idx] = PMU_EVENT_IDX_INVALID;
                        }
                        result
                    };
                    match stop_result {
                        Ok(_) => result = SbiRet::success(0),
                        Err(error) => return hardware_error(error),
                    }
                }
                result
            })
        })
    }

    /// Reads the low XLEN bits of a configured firmware counter (FID #5).
    fn counter_fw_read(&self, counter_idx: usize) -> SbiRet {
        with_current(|local| {
            local.with_pmu(|state| match state.firmware_counter_value(counter_idx) {
                Some(value) => SbiRet::success(value as usize),
                None => SbiRet::invalid_param(),
            })
        })
    }

    /// Reads a configured firmware counter's high 32 bits on RV32, or zero on RV64 (FID #6).
    fn counter_fw_read_hi(&self, counter_idx: usize) -> SbiRet {
        with_current(|local| {
            local.with_pmu(|state| match state.firmware_counter_value(counter_idx) {
                Some(value) => SbiRet::success(if cfg!(target_pointer_width = "32") {
                    (value >> 32) as usize
                } else {
                    0
                }),
                None => SbiRet::invalid_param(),
            })
        })
    }

    /// Reports that optional PMU snapshot shared memory is unsupported (FID #7).
    fn snapshot_set_shmem(&self, shmem: SharedPtr<[u8; SIZE]>, flags: usize) -> SbiRet {
        let _ = (shmem, flags);
        SbiRet::not_supported()
    }
}

/// Translates hardware failures at the SBI policy boundary.
fn hardware_error(error: CounterError) -> SbiRet {
    match error {
        CounterError::InvalidCounter | CounterError::TimeCounter | CounterError::WrongHart => {
            SbiRet::invalid_param()
        }
        CounterError::AlreadyStarted => SbiRet::already_started(),
        CounterError::AlreadyStopped => SbiRet::already_stopped(),
        CounterError::InvalidHartId => {
            error!("PMU operation on a hart outside the boot topology");
            SbiRet::failed()
        }
        CounterError::Access(error) => {
            error!("PMU hardware operation failed: {:?}", error);
            SbiRet::failed()
        }
        CounterError::RestoreFailed {
            operation,
            restoration,
        } => {
            error!(
                "PMU configuration failed: {:?}; restoring its previous state failed: {:?}",
                operation, restoration
            );
            SbiRet::failed()
        }
    }
}

/// Translates event-selection failures only at the SBI boundary.
fn event_error(error: EventError) -> SbiRet {
    match error {
        EventError::Unavailable => SbiRet::not_supported(),
        EventError::Counter(error) => hardware_error(error),
    }
}

/// SBI encoding of a counter's type, readable CSR, and width minus one.
struct CounterInfo {
    /// Packed counter information.
    ///
    /// - Bits `[11:0]`: CSR number for hardware counters.
    /// - Bits `[17:12]`: Counter width minus one.
    /// - MSB: Set for firmware counters, clear for hardware counters.
    inner: usize,
}

impl CounterInfo {
    const CSR_MASK: usize = 0xFFF;
    const FIRMWARE_FLAG: usize = 1 << (usize::BITS as usize - 1);

    pub const fn with_hardware_info(csr_num: u16, width: u8) -> Self {
        Self {
            inner: ((csr_num as usize) & Self::CSR_MASK) | (((width as usize) & 0x3F) << 12),
        }
    }

    pub const fn with_firmware_info() -> Self {
        Self {
            inner: Self::FIRMWARE_FLAG,
        }
    }

    pub const fn inner(self) -> usize {
        self.inner
    }
}
