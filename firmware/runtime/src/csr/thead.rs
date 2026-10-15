//! T-Head CSR operations used during per-hart startup.

use csr::thead::machine::ExtendedStatus;

/// Disables non-standard page memory attributes on a T-Head hart.
///
/// Other vendors need no change.
///
/// # Safety
///
/// Run during M-mode startup after Runtime trap entry is installed, with
/// `mscratch` zero and `MPRV` clear. No lower-mode address space may be active.
pub unsafe fn disable_maee() {
    const THEAD_VENDOR_ID: usize = 0x5b7;
    if riscv::register::mvendorid::read().bits() != THEAD_VENDOR_ID {
        return;
    }
    // Runtime entry recovers an unavailable mxstatus before any MAEE update.
    if super::instruction!("csrr {value}, {csr}", ExtendedStatus::NUMBER, 0).is_ok() {
        // SAFETY: The caller owns translation setup and the CSR probe succeeded.
        unsafe { clear_maee() };
    }
}

/// Clears MAEE while preserving every other mxstatus field.
///
/// # Safety
///
/// Run in M-mode on a T-Head hart with mxstatus, before enabling translation.
unsafe fn clear_maee() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: The caller establishes CSR support and owns translation setup.
    unsafe {
        core::arch::asm!(
            "csrc {csr}, {mask}",
            csr = const ExtendedStatus::NUMBER,
            mask = in(reg) 1usize << 21,
            options(nomem),
        );
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("CSR access requires RISC-V");
}
