use sbi_spec::binary::SbiRet;

#[cfg(target_pointer_width = "64")]
#[inline]
pub fn handle_ecall(
    function: usize,
    param0: usize,
    param1: usize,
    param2: usize,
    param3: usize,
    param4: usize,
) -> SbiRet {
    use crate::pmu::*;
    use sbi_spec::pmu::*;
    match function {
        PMU_NUM_COUNTERS => num_counters(),
        PMU_COUNTER_GET_INFO => counter_get_info(param0),
        PMU_COUNTER_CONFIG_MATCHING => {
            counter_config_matching(param0, param1, param2, param3, param4 as _)
        }
        PMU_COUNTER_START => counter_start(param0, param1, param2, param3 as _),
        PMU_COUNTER_STOP => counter_stop(param0, param1, param2),
        PMU_COUNTER_FW_READ => counter_fw_read(param0),
        _ => SbiRet::not_supported(),
    }
}

#[cfg(target_pointer_width = "32")]
#[inline]
pub fn handle_ecall(
    function: usize,
    param0: usize,
    param1: usize,
    param2: usize,
    param3: usize,
    param4: usize,
    param5: usize,
) -> SbiRet {
    use super::concat_u32;
    use crate::pmu::*;
    use sbi_spec::pmu::*;
    match function {
        PMU_NUM_COUNTERS => num_counters(),
        PMU_COUNTER_GET_INFO => counter_get_info(param0),
        PMU_COUNTER_CONFIG_MATCHING => {
            counter_config_matching(param0, param1, param2, param3, concat_u32(param5, param4))
        }
        PMU_COUNTER_START => counter_start(param0, param1, param2, concat_u32(param4, param3)),
        PMU_COUNTER_STOP => counter_stop(param0, param1, param2),
        PMU_COUNTER_FW_READ => counter_fw_read(param0),
        _ => SbiRet::not_supported(),
    }
}
