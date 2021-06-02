//! hsm extension
use super::SbiRet;

const FUNCTION_HSM_HART_START: usize = 0x0;
const FUNCTION_HSM_HART_STOP: usize = 0x1;
const FUNCTION_HSM_HART_GET_STATUS: usize = 0x2;

#[inline]
pub fn handle_ecall_hsm(function: usize, param0: usize, param1: usize, param2: usize) -> SbiRet {
    match function {
        FUNCTION_HSM_HART_START => hart_start(param0, param1, param2),
        FUNCTION_HSM_HART_STOP => hart_stop(param0),
        FUNCTION_HSM_HART_GET_STATUS => hart_get_status(param0),
        _ => SbiRet::not_supported(),
    }
}

#[inline]
fn hart_start(hartid: usize, start_addr: usize, private_value: usize) -> SbiRet {
    crate::hsm::hart_start(hartid, start_addr, private_value)
}

#[inline]
fn hart_stop(hartid: usize) -> SbiRet {
    crate::hsm::hart_stop(hartid)
}

#[inline]
fn hart_get_status(hartid: usize) -> SbiRet {
    crate::hsm::hart_get_status(hartid)
}
