//! Andes CSR definitions and AX45MP/A27L2 cache-control hardware access.
//!
//! The [AX45MP-1C Processor Reference Manual](https://www.andestech.com/wp-content/uploads/AX45MP-1C-Rev.-5.0.0-Datasheet.pdf),
//! §§7.6–7.7 and §16.12.14, defines the cache operations and their privilege requirements.

use core::arch::asm;

pub use riscv_csr_spec::vendor::andes::*;

/// Issues `operation` for the cache line containing `address`.
///
/// The command takes a virtual address in the hart's current address space.
///
/// # Safety
///
/// 1. Establish that the hart implements the selected CCTL command and permits
///    access to both ports at the current privilege level.
///    S-mode and U-mode access requires `mcache_ctl.CCTL_SUEN`.
/// 2. Exclude competing uses of the address and command ports until the command is issued.
/// 3. Coordinate access to the entire line, including DMA, so that discarding
///    dirty bytes or writing them back preserves all live Rust values and references.
#[inline(always)]
pub unsafe fn maintain_line(address: usize, operation: CacheLineOperation) {
    // SAFETY: The caller owns the CCTL address/command pair and affected memory range.
    unsafe {
        asm!(
            "fence",
            "csrw {addr_csr}, {addr}",
            "csrw {cmd_csr}, {cmd}",
            addr_csr = const Ucctlbeginaddr::NUMBER,
            cmd_csr = const Ucctlcommand::NUMBER,
            addr = in(reg) address,
            cmd = in(reg) operation as usize,
            options(nostack),
        );
    }
}

/// Issues a writeback and invalidation of the entire L1 data cache.
///
/// # Safety
///
/// 1. Execute on a core implementing this command, in M-mode or in S-mode
///    with `mcache_ctl.CCTL_SUEN` set.
/// 2. Own this hart's command port and coordinate concurrent device writes.
///    Writeback to every affected mapping must preserve live Rust values and references.
#[inline(always)]
pub unsafe fn write_back_and_invalidate_all() {
    // SAFETY: The caller owns the CCTL command port and permits whole-cache maintenance.
    unsafe {
        asm!(
            "fence",
            "csrw {cmd_csr}, {cmd}",
            cmd_csr = const Ucctlcommand::NUMBER,
            cmd = in(reg) Ucctlcommand::L1D_WBINVAL_ALL,
            options(nostack),
        );
    }
}
