use super::SbiRet;
use crate::hart_mask::HartMask;
use crate::ipi::send_ipi_many;

const FUNCTION_IPI_SEND_IPI: usize = 0x0;

#[inline]
pub fn handle_ecall_ipi(function: usize, param0: usize, param1: usize) -> SbiRet {
    match function {
        FUNCTION_IPI_SEND_IPI => send_ipi(param0, param1),
        _ => SbiRet::not_supported(),
    }
}

#[inline]
fn send_ipi(hart_mask: usize, hart_mask_base: usize) -> SbiRet {
    let hart_mask = HartMask::from_mask_base(hart_mask, hart_mask_base);
    send_ipi_many(hart_mask)
}
