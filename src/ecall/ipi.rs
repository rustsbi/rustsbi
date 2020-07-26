use super::SbiRet;

const FUNCTION_IPI_SEND_IPI: usize = 0x0;

#[inline]
pub fn handle_ecall_ipi(function: usize, param0: usize, param1: usize) -> SbiRet {
    match function {
        FUNCTION_IPI_SEND_IPI => send_ipi(param0, param1),
        _ => SbiRet::not_supported(),
    }
}

#[inline]
fn send_ipi(_hart_mask: usize, _hart_mask_base: usize) -> SbiRet {
    // todo: send software interrupt to another hart
    SbiRet::ok(0)
}
