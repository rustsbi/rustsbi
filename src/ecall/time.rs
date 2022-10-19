use sbi_spec::binary::SbiRet;

#[cfg(target_pointer_width = "64")]
#[inline]
pub(super) fn handle_ecall(function: usize, param0: usize) -> SbiRet {
    use crate::timer::*;
    use sbi_spec::time::*;
    match function {
        SET_TIMER => {
            if set_timer(param0 as _) {
                SbiRet::success(0)
            } else {
                SbiRet::not_supported()
            }
        }
        _ => SbiRet::not_supported(),
    }
}

#[cfg(target_pointer_width = "32")]
#[inline]
pub(super) fn handle_ecall(function: usize, param0: usize, param1: usize) -> SbiRet {
    use super::concat_u32;
    use crate::timer::*;
    use sbi_spec::time::*;
    match function {
        SET_TIMER => {
            if set_timer(concat_u32(param1, param0)) {
                SbiRet::success(0)
            } else {
                SbiRet::not_supported()
            }
        }
        _ => SbiRet::not_supported(),
    }
}
