//! Position-independent ELF relative relocation before Rust initialization.

use crate::config::SBI_LINK_START_ADDRESS;

const R_RISCV_RELATIVE: usize = 3;

#[cfg(target_pointer_width = "64")]
/// Applies the linker's relative relocations before Rust memory is initialized.
///
/// # Safety
///
/// The raw entry path must call this exactly once before BSS, stacks, or Rust
/// references are used. The linker must provide ordered, in-image
/// `__rel_dyn_start..__rel_dyn_end` entries with the ELF64 RISC-V layout; every
/// `R_RISCV_RELATIVE` destination must be aligned, writable image storage and
/// its addend must resolve inside the linked execution envelope.
#[unsafe(naked)]
pub(super) unsafe extern "C" fn relocation_update() {
    core::arch::naked_asm!(
        "li t0, {link_start}",
        "lla t1, sbi_start",
        "sub t2, t1, t0",
        "lla t0, __rel_dyn_start",
        "lla t1, __rel_dyn_end",
        "li t3, {relative}",
        "1:",
        "bgeu t0, t1, 2f",
        "ld t4, 8(t0)",
        "bne t4, t3, 3f",
        "ld t4, 0(t0)",
        "ld t5, 16(t0)",
        "add t4, t4, t2",
        "add t5, t5, t2",
        "sd t5, 0(t4)",
        "3:",
        "addi t0, t0, 24",
        "j 1b",
        "2:",
        "fence.i",
        "ret",
        link_start = const SBI_LINK_START_ADDRESS,
        relative = const R_RISCV_RELATIVE,
    )
}

#[cfg(target_pointer_width = "32")]
/// Applies the linker's relative relocations before Rust memory is initialized.
///
/// # Safety
///
/// The raw entry path must call this exactly once before BSS, stacks, or Rust
/// references are used. The linker must provide ordered, in-image
/// `__rel_dyn_start..__rel_dyn_end` entries with the ELF32 RISC-V layout; every
/// `R_RISCV_RELATIVE` destination must be aligned, writable image storage and
/// its addend must resolve inside the linked execution envelope.
#[unsafe(naked)]
pub(super) unsafe extern "C" fn relocation_update() {
    core::arch::naked_asm!(
        "li t0, {link_start}",
        "lla t1, sbi_start",
        "sub t2, t1, t0",
        "lla t0, __rel_dyn_start",
        "lla t1, __rel_dyn_end",
        "li t3, {relative}",
        "1:",
        "bgeu t0, t1, 2f",
        "lw t4, 4(t0)",
        "bne t4, t3, 3f",
        "lw t4, 0(t0)",
        "lw t5, 8(t0)",
        "add t4, t4, t2",
        "add t5, t5, t2",
        "sw t5, 0(t4)",
        "3:",
        "addi t0, t0, 12",
        "j 1b",
        "2:",
        "fence.i",
        "ret",
        link_start = const SBI_LINK_START_ADDRESS,
        relative = const R_RISCV_RELATIVE,
    )
}
