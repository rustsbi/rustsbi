use super::SbiRet;
use crate::hart_mask::HartMask;
use crate::ipi::{max_hart_id, send_ipi_many};

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
    let max_hart_id = if let Some(id) = max_hart_id() {
        id
    } else {
        return SbiRet::not_supported()
    };
    let hart_mask = unsafe { HartMask::from_addr(hart_mask, hart_mask_base, max_hart_id) };
    send_ipi_many(hart_mask)
}
