//! Typed cold/warm Rust transfer and embedded next-stage inputs.

use core::sync::atomic::Ordering;

use crate::boot::BootInfo;

use super::state::{
    EARLY_FAILED, EARLY_READY, EARLY_STATE, RUNTIME_FAILED, RUNTIME_READY, RUNTIME_STATE,
};

pub(super) extern "C" fn warm_entry(hart_id: usize, index: usize) -> ! {
    crate::boot::enter_warm_hart(hart_id, index)
}

pub(crate) fn publish_runtime() {
    RUNTIME_STATE.store(RUNTIME_READY, Ordering::Release);
}

pub(crate) fn fail_runtime() {
    RUNTIME_STATE.store(RUNTIME_FAILED, Ordering::Release);
}
unsafe extern "Rust" {
    safe fn __rustsbi_prototyper_main(boot: BootInfo) -> !;
}

pub(super) fn enter_policy(boot: BootInfo) -> ! {
    EARLY_STATE.store(EARLY_READY, Ordering::Release);
    __rustsbi_prototyper_main(boot)
}

pub(super) fn fail_stop() -> ! {
    EARLY_FAILED.store(1, Ordering::Release);
    loop {
        // SAFETY: interrupts remain disabled and this terminal path owns no
        // live Rust borrow that could be observed after wakeup.
        unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
    }
}
