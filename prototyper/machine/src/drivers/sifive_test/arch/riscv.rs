//! RISC-V device ordering for the SiFive test register.

pub(in crate::drivers::sifive_test) fn device_fence() {
    // SAFETY: orders the exclusively owned power register transaction.
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) }
}
