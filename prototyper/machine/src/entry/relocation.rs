//! Position-independent ELF relative relocation before Rust initialization.

use crate::config::SBI_LINK_START_ADDRESS;

const R_RISCV_RELATIVE: usize = 3;

/// Applies the linker's relative relocations before Rust memory is initialized.
///
/// # Safety
///
/// Raw entry calls this exactly once before BSS, stacks, or Rust references
/// are used. The linker provides the matching ELF32/ELF64 RISC-V relocation
/// layout and writable in-image destinations.
#[unsafe(naked)]
pub(super) unsafe extern "C" fn relocation_update() {
    core::arch::naked_asm!(
        include_str!("relocation.S"),
        word_size = const core::mem::size_of::<usize>(),
        link_start = const SBI_LINK_START_ADDRESS,
        relative = const R_RISCV_RELATIVE,
    )
}
