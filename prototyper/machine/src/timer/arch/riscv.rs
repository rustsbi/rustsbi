//! RISC-V timer identity access.

pub(in crate::timer) fn current_hart_id() -> usize {
    let hart_id;
    // SAFETY: `mhartid` is a mandatory read-only machine CSR.
    unsafe {
        core::arch::asm!("csrr {hart_id}, mhartid", hart_id = out(reg) hart_id, options(nomem, nostack))
    };
    hart_id
}
