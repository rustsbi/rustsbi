//! RISC-V instructions used by firmware for synchronization and idle waits.
//!
//! See <https://docs.riscv.org/reference/isa/unpriv/rv32.html>,
//! <https://docs.riscv.org/reference/isa/unpriv/zifencei.html>, and
//! <https://docs.riscv.org/reference/isa/priv/supervisor.html#_supervisor_memory_management_fence_instruction>.
//! WFI is defined in <https://docs.riscv.org/reference/isa/priv/machine.html>.

/// Orders all memory and device accesses around the instruction.
#[inline]
pub fn fence() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: A fence only orders accesses.
    unsafe {
        core::arch::asm!("fence", options(nostack))
    };
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("RISC-V architecture required");
}

/// Synchronizes the instruction stream with prior stores on this hart.
///
/// This instruction traps if the hart does not implement Zifencei.
#[inline]
pub fn fence_i() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: The instruction synchronizes instruction fetches without
    // accessing Rust memory operands. Unsupported execution traps.
    unsafe {
        core::arch::asm!("fence.i", options(nostack))
    };
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("RISC-V architecture required");
}

/// Waits for an interrupt or returns when the implementation chooses.
///
/// Execution may trap when the current privilege mode cannot use WFI.
#[inline]
pub fn wait_for_interrupt() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: WFI does not access memory or change registers. If execution is
    // restricted in the current privilege mode, it traps.
    unsafe {
        core::arch::asm!("wfi", options(nomem, nostack))
    };
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("RISC-V architecture required");
}

/// Synchronizes all supervisor address translations on this hart.
///
/// Execution traps if the instruction is unavailable at the current privilege.
#[inline]
pub fn sfence_vma_all() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: The instruction synchronizes address translation without
    // accessing Rust memory operands. Unsupported execution traps.
    unsafe {
        core::arch::asm!("sfence.vma", options(nostack))
    };
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("RISC-V architecture required");
}

/// Synchronizes one address and ASID on this hart.
///
/// Execution traps if the instruction is unavailable at the current privilege.
#[inline]
pub fn sfence_vma(asid: usize, address: usize) {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: The instruction synchronizes address translation without
    // accessing Rust memory operands. Unsupported execution traps.
    unsafe {
        core::arch::asm!("sfence.vma {}, {}", in(reg) address, in(reg) asid, options(nostack))
    };
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = (asid, address);
        unimplemented!("RISC-V architecture required")
    }
}
