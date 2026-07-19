//! RISC-V UART device ordering.

pub(in crate::drivers::uart) fn io_fence() {
    // SAFETY: orders only the calling hart's device input/output.
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack, preserves_flags)) }
}
