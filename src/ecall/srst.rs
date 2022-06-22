use sbi_spec::binary::SbiRet;

#[inline]
pub(super) fn handle_ecall(function: usize, param0: usize, param1: usize) -> SbiRet {
    use crate::reset::*;
    use sbi_spec::srst::*;
    match function {
        SYSTEM_RESET => match (u32::try_from(param0), u32::try_from(param1)) {
            (Ok(reset_type), Ok(reset_reason)) => system_reset(reset_type, reset_reason),
            (_, _) => SbiRet::invalid_param(),
        },
        _ => SbiRet::not_supported(),
    }
}
