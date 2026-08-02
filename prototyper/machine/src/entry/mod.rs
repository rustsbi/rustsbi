//! Raw image entry, relocation, stacks, and the one Rust policy transfer.

mod allocator;
mod dynamic;
mod fixed;
pub(crate) mod image;
mod relocation;
mod stacks;

use core::sync::atomic::{AtomicU32, Ordering};

pub(crate) use stacks::{enter_warm_loop, hart_stack_top};

const EARLY_UNCLAIMED: u32 = 0;
pub(super) const EARLY_INITIALIZING: u32 = 1;
pub(super) const EARLY_READY: u32 = 2;
const RUNTIME_WAITING: u32 = 0;
pub(super) const RUNTIME_READY: u32 = 1;
pub(super) const RUNTIME_FAILED: u32 = 2;

#[used]
#[unsafe(link_section = ".data.entry")]
pub(super) static EARLY_STATE: AtomicU32 = AtomicU32::new(EARLY_UNCLAIMED);

#[used]
#[unsafe(link_section = ".data.entry")]
pub(super) static EARLY_FAILED: AtomicU32 = AtomicU32::new(0);

pub(super) static RUNTIME_STATE: AtomicU32 = AtomicU32::new(RUNTIME_WAITING);

/// Enters the raw mechanism selected by the linked firmware contract.
///
/// # Safety
///
/// The previous stage must supply the register envelope required by the
/// selected standard firmware type.
#[unsafe(naked)]
#[unsafe(export_name = "__rustsbi_prototyper_from_previous")]
pub unsafe extern "C" fn raw_entry() -> ! {
    core::arch::naked_asm!(
        "lla t0, __prototyper_contract_start",
        "lbu t0, 6(t0)",
        "li t1, 1",
        "beq t0, t1, 1f",
        "li t1, 2",
        "beq t0, t1, 2f",
        "li t1, 3",
        "beq t0, t1, 2f",
        "3:",
        "wfi",
        "j 3b",
        "1:",
        "j {dynamic}",
        "2:",
        "j {fixed}",
        dynamic = sym dynamic::entry,
        fixed = sym fixed::entry,
    )
}

pub(super) extern "C" fn warm_entry(hart_id: usize, index: usize) -> ! {
    crate::hart::run_warm_hart(hart_id, index)
}

pub(crate) fn publish() {
    RUNTIME_STATE.store(RUNTIME_READY, Ordering::Release);
}

pub(crate) fn fail() {
    RUNTIME_STATE.store(RUNTIME_FAILED, Ordering::Release);
}

unsafe extern "Rust" {
    safe fn __rustsbi_prototyper_main(boot: crate::boot::BootInfo) -> !;
}

pub(super) fn enter_policy(boot: crate::boot::BootInfo) -> ! {
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
