use sbi_spec::binary::SbiRet;

#[inline]
pub(super) fn handle_ecall(function: usize, param0: usize, param1: usize) -> SbiRet {
    use crate::hart_mask::HartMask;
    use crate::ipi::*;
    use sbi_spec::spi::*;
    match function {
        SEND_IPI => send_ipi(HartMask::from_mask_base(param0, param1)),
        _ => SbiRet::not_supported(),
    }
}
