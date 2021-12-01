//! pmu extension
use super::SbiRet;
use crate::pmu;

const FUNCTION_PMU_NUM_COUNTERS: usize = 0x0;
const FUNCTION_PMU_COUNTER_GET_INFO: usize = 0x1;
const FUNCTION_PMU_COUNTER_CONFIG_MATCHING: usize = 0x2;
const FUNCTION_PMU_COUNTER_START: usize = 0x3;
const FUNCTION_PMU_COUNTER_STOP: usize = 0x4;
const FUNCTION_PMU_COUNTER_FW_READ: usize = 0x5;

#[inline]
#[cfg(target_pointer_width = "64")]
pub fn handle_ecall_pmu_64(
    function: usize,
    param0: usize,
    param1: usize,
    param2: usize,
    param3: usize,
    param4: usize,
) -> SbiRet {
    match function {
        FUNCTION_PMU_NUM_COUNTERS => pmu::num_counters(),
        FUNCTION_PMU_COUNTER_GET_INFO => pmu::counter_get_info(param0),
        FUNCTION_PMU_COUNTER_CONFIG_MATCHING => {
            counter_config_matching_64(param0, param1, param2, param3, param4)
        }
        FUNCTION_PMU_COUNTER_START => counter_start_64(param0, param1, param2, param3),
        FUNCTION_PMU_COUNTER_STOP => pmu::counter_stop(param0, param1, param2),
        FUNCTION_PMU_COUNTER_FW_READ => pmu::counter_fw_read(param0),
        _ => SbiRet::not_supported(),
    }
}

#[inline]
#[cfg(target_pointer_width = "32")]
pub fn handle_ecall_pmu_32(
    function: usize,
    param0: usize,
    param1: usize,
    param2: usize,
    param3: usize,
    param4: usize,
    param5: usize,
) -> SbiRet {
    match function {
        FUNCTION_PMU_NUM_COUNTERS => pmu::num_counters(),
        FUNCTION_PMU_COUNTER_GET_INFO => pmu::counter_get_info(param0),
        FUNCTION_PMU_COUNTER_CONFIG_MATCHING => {
            counter_config_matching_32(param0, param1, param2, param3, param4, param5)
        }
        FUNCTION_PMU_COUNTER_START => counter_start_32(param0, param1, param2, param3, param4),
        FUNCTION_PMU_COUNTER_STOP => pmu::counter_stop(param0, param1, param2),
        FUNCTION_PMU_COUNTER_FW_READ => pmu::counter_fw_read(param0),
        _ => SbiRet::not_supported(),
    }
}

#[cfg(target_pointer_width = "64")]
#[inline]
fn counter_config_matching_64(
    counter_idx_base: usize,
    counter_idx_mask: usize,
    config_flags: usize,
    event_idx: usize,
    event_data: usize,
) -> SbiRet {
    pmu::counter_config_matching(
        counter_idx_base,
        counter_idx_mask,
        config_flags,
        event_idx,
        event_data as u64,
    )
}

#[cfg(target_pointer_width = "32")]
#[inline]
fn counter_config_matching_32(
    counter_idx_base: usize,
    counter_idx_mask: usize,
    config_flags: usize,
    event_idx: usize,
    event_data_lo: usize,
    event_data_hi: usize,
) -> SbiRet {
    let event_data = (event_data_lo as u64) + ((event_data_hi as u64) << 32);
    pmu::counter_config_matching(
        counter_idx_base,
        counter_idx_mask,
        config_flags,
        event_idx,
        event_data,
    )
}

#[cfg(target_pointer_width = "64")]
#[inline]
fn counter_start_64(
    counter_idx_base: usize,
    counter_idx_mask: usize,
    start_flags: usize,
    initial_value: usize,
) -> SbiRet {
    pmu::counter_start(
        counter_idx_base,
        counter_idx_mask,
        start_flags,
        initial_value as u64,
    )
}

#[cfg(target_pointer_width = "32")]
#[inline]
fn counter_start_32(
    counter_idx_base: usize,
    counter_idx_mask: usize,
    start_flags: usize,
    initial_value_lo: usize,
    initial_value_hi: usize,
) -> SbiRet {
    let initial_value = (initial_value_lo as u64) + ((initial_value_hi as u64) << 32);
    pmu::counter_start(
        counter_idx_base,
        counter_idx_mask,
        start_flags,
        initial_value,
    )
}
