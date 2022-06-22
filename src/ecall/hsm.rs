use sbi_spec::binary::SbiRet;

#[inline]
pub(super) fn handle_ecall(function: usize, param0: usize, param1: usize, param2: usize) -> SbiRet {
    use crate::hsm::*;
    use sbi_spec::hsm::*;
    match function {
        HART_START => hart_start(param0, param1, param2),
        HART_STOP => hart_stop(),
        HART_GET_STATUS => hart_get_status(param0),
        HART_SUSPEND => {
            if let Ok(suspend_type) = u32::try_from(param0) {
                hart_suspend(suspend_type, param1, param2)
            } else {
                SbiRet::invalid_param()
            }
        }
        _ => SbiRet::not_supported(),
    }
}
