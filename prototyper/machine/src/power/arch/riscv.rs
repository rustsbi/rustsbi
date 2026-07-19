//! RISC-V terminal machine-state operations.

pub(crate) fn mask_local_interrupts() {
    // SAFETY: terminal transition changes only local `mie` and never resumes.
    unsafe { core::arch::asm!("csrw mie, zero", options(nostack)) }
}

pub(crate) fn halt() -> ! {
    // Architecture invariant: every local source is masked before waiting.
    unsafe {
        core::arch::asm!("csrw mie, zero", options(nostack));
        loop {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}
