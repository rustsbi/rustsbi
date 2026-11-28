//! Hypervisor CSR definitions and exception delegation access.

pub use riscv_csr_spec::standard::isa::privileged::hypervisor::*;
use riscv_csr_spec::standard::isa::values::trap::Exception;

/// Returns whether a guest exception is delegated by `hedeleg`.
///
/// Returns `Ok(false)` for codes above 63 without reading a CSR.
/// On RV32, a code in `32..64` returns an access error if `hedelegh` traps as illegal.
///
/// # Safety
///
/// For a code below 64, execute on a hart with H at a privilege level that can read `hedeleg`.
/// For RV32 codes in `32..64`, establish the M-mode [recovery ABI](crate::recovery).
#[inline(always)]
pub unsafe fn read_hedeleg_bit(exception: Exception) -> Result<bool, crate::AccessError> {
    let code = exception.code();
    if code >= u64::BITS as usize {
        return Ok(false);
    }
    #[cfg(target_pointer_width = "32")]
    if code >= u32::BITS as usize {
        use crate::Read;

        // SAFETY: The caller establishes recovery for the optional upper word.
        let high = unsafe { Hedelegh::try_read()? };
        return Ok(high & (1 << (code - u32::BITS as usize)) != 0);
    }
    let word: usize;
    // SAFETY: The caller establishes that the primary delegation CSR is readable.
    unsafe {
        core::arch::asm!("csrr {}, {}", out(reg) word, const Hedeleg::NUMBER, options(nostack))
    };
    Ok(word & (1 << code) != 0)
}
