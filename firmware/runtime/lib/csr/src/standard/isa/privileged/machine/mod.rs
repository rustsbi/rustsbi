//! Machine-level CSR definitions and hardware access.

pub use riscv_csr_spec::standard::isa::privileged::machine::*;

#[macro_use]
mod counter_setup;
mod counter_timer;
mod pmp;
mod trap_setup;

pub use counter_setup::try_write_mhpmevent;
#[cfg(target_pointer_width = "32")]
pub use counter_setup::{try_read_mhpmeventh, try_write_mhpmeventh};
pub use counter_timer::{measure_stopped_mhpmcounter, write_mhpmcounter};
pub use pmp::{read_pmpaddr, read_pmpcfg, write_pmpaddr, write_pmpcfg};
pub use trap_setup::{read_medeleg_word, write_medeleg_word};

/// Returns whether the selected `menvcfg` bit is set.
///
/// On RV32, only the word containing the bit is accessed.
///
/// # Safety
///
/// Establish the M-mode trap-recovery requirements of [`crate::Read::try_read`].
#[inline(always)]
pub unsafe fn try_read_menvcfg_bit<const BIT: u32>(
    _field: MenvcfgBit<BIT>,
) -> Result<bool, crate::AccessError> {
    let word: usize;
    // SAFETY: The caller establishes recovery for this single standard CSR read.
    unsafe {
        crate::recovery::try_csr_asm!(
            "csrr {value}, {csr}",
            <Menvcfg as riscv_csr_spec::WriteSpec<crate::Set<MenvcfgBit<BIT>>>>::ATOMIC_NUMBER,
            value = out(reg) word,
        )
    }?;
    Ok(word & (1usize << (BIT % usize::BITS)) != 0)
}
