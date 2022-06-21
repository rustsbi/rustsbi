use super::SbiRet;

const FUNCTION_SYSTEM_RESET: u32 = 0x0;

#[inline]
pub fn handle_ecall_srst(function: u32, param0: usize, param1: usize) -> SbiRet {
    match function {
        FUNCTION_SYSTEM_RESET => system_reset(param0 as _, param1 as _),
        _ => SbiRet::not_supported(),
    }
}

#[inline]
fn system_reset(reset_type: u32, reset_reason: u32) -> SbiRet {
    crate::reset::system_reset(reset_type, reset_reason)
}
