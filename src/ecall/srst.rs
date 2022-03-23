use super::SbiRet;

const FUNCTION_SYSTEM_RESET: u32 = 0x0;

#[inline]
pub fn handle_ecall_srst(function: u32, param0: usize, param1: usize) -> SbiRet {
    match function {
        FUNCTION_SYSTEM_RESET => system_reset(param0, param1),
        _ => SbiRet::not_supported(),
    }
}

#[inline]
fn system_reset(reset_type: usize, reset_reason: usize) -> SbiRet {
    crate::reset::system_reset(reset_type, reset_reason)
}
