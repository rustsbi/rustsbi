//! Chapter 7. IPI Extension (EID #0x735049 "sPI: s-mode IPI")

use crate::binary::sbi_call_2;

use sbi_spec::{
    binary::{HartMask, SbiRet},
    spi::{EID_SPI, SEND_IPI},
};

/// Send an inter-processor interrupt to all harts defined in hart mask.
///
/// Inter-processor interrupts manifest at the receiving harts as the supervisor software interrupts.
///
/// # Return value
///
/// Should return `SbiRet::success()` if IPI was sent to all the targeted harts successfully.
///
/// This function is defined in RISC-V SBI Specification chapter 7.1.
#[inline]
pub fn send_ipi(hart_mask: HartMask) -> SbiRet {
    let (hart_mask, hart_mask_base) = hart_mask.into_inner();
    sbi_call_2(EID_SPI, SEND_IPI, hart_mask, hart_mask_base)
}
