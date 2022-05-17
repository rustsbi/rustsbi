//! hsm extension
use super::SbiRet;

const FUNCTION_HSM_HART_START: u32 = 0x0;
const FUNCTION_HSM_HART_STOP: u32 = 0x1;
const FUNCTION_HSM_HART_GET_STATUS: u32 = 0x2;
const FUNCTION_HSM_HART_SUSPEND: u32 = 0x3;

#[inline]
pub fn handle_ecall_hsm(function: u32, param0: usize, param1: usize, param2: usize) -> SbiRet {
    match function {
        FUNCTION_HSM_HART_START => hart_start(param0, param1, param2),
        FUNCTION_HSM_HART_STOP => hart_stop(),
        FUNCTION_HSM_HART_GET_STATUS => hart_get_status(param0),
        FUNCTION_HSM_HART_SUSPEND => hart_suspend(param0, param1, param2),
        _ => SbiRet::not_supported(),
    }
}

#[inline]
fn hart_start(hartid: usize, start_addr: usize, opaque: usize) -> SbiRet {
    crate::hsm::hart_start(hartid, start_addr, opaque)
}

#[inline]
fn hart_stop() -> SbiRet {
    crate::hsm::hart_stop()
}

#[inline]
fn hart_get_status(hartid: usize) -> SbiRet {
    crate::hsm::hart_get_status(hartid)
}

#[inline]
fn hart_suspend(suspend_type: usize, resume_addr: usize, opaque: usize) -> SbiRet {
    if suspend_type > u32::MAX as usize {
        // valid suspend type should be a `u32` typed value
        return SbiRet::invalid_param();
    }
    crate::hsm::hart_suspend(suspend_type as u32, resume_addr, opaque)
}
