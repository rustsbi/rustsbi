//! Primary-word exception delegation, including harts without `medelegh`.

use crate::standard::isa::{privileged::machine::Medeleg, values::trap::ExceptionSet};

/// Reads the native word of `medeleg`, zero-extending it on RV32.
#[inline(always)]
pub fn read_medeleg_word() -> ExceptionSet {
    let word: usize;
    // SAFETY: Reading the standard primary delegation CSR has no read side effects.
    unsafe {
        core::arch::asm!("csrr {}, {}", out(reg) word, const Medeleg::NUMBER, options(nostack))
    };
    ExceptionSet::from_bits(word as u64)
}

/// Writes the native word of `medeleg`.
///
/// Read back the bits to determine which exceptions the hart actually delegates.
/// On RV32 this preserves `medelegh`.
///
/// # Safety
///
/// Execute in M-mode on a hart implementing `medeleg`.
/// Own this hart's trap routing and preserve any delegation that live contexts rely on.
#[inline(always)]
pub unsafe fn write_medeleg_word(exceptions: ExceptionSet) {
    let word = exceptions.bits() as usize;
    // SAFETY: The caller owns this hart's primary delegation-word update.
    unsafe {
        core::arch::asm!("csrw {}, {}", const Medeleg::NUMBER, in(reg) word, options(nostack))
    };
}
