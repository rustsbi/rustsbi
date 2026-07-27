//! Boot and per-hart stack storage owned by machine startup.

use core::cell::UnsafeCell;

use crate::config::{BOOT_STACK_SIZE, HART_CAPACITY};

#[repr(C, align(16))]
pub(super) struct BootStack(pub(super) UnsafeCell<[u8; BOOT_STACK_SIZE]>);

// SAFETY: no Rust reference to the stack storage is ever created. Raw entry
// gives its address only to the unique initialization hart after BSS clearing,
// and that hart retains exclusive stack ownership until the terminal handoff.
unsafe impl Sync for BootStack {}

#[used]
#[unsafe(link_section = ".bss.stack")]
pub(super) static BOOT_STACK: BootStack = BootStack(UnsafeCell::new([0; BOOT_STACK_SIZE]));

#[repr(C, align(16))]
pub(super) struct HartStack(pub(super) UnsafeCell<[u8; BOOT_STACK_SIZE]>);

// SAFETY: raw warm entry assigns each stack only after resolving a distinct
// dense hart index from the immutable published map. Stack storage is never
// exposed as a Rust reference.
unsafe impl Sync for HartStack {}

#[used]
#[unsafe(link_section = ".bss.hart_stack")]
pub(super) static HART_STACKS: [HartStack; HART_CAPACITY] =
    [const { HartStack(UnsafeCell::new([0; BOOT_STACK_SIZE])) }; HART_CAPACITY];

pub(crate) fn hart_stack_top(index: usize) -> Option<usize> {
    let stack = HART_STACKS.get(index)?;
    (stack.0.get() as usize).checked_add(BOOT_STACK_SIZE)
}

/// Switches to a validated permanent hart stack and enters the warm loop.
///
/// # Safety
///
/// `stack_top` must be the disjoint top returned for `index`; all references on
/// the current stack must be terminally abandoned, and interrupts must remain
/// disabled until the warm loop completes local preparation.
pub(crate) unsafe fn enter_warm_loop(hart_id: usize, index: usize, stack_top: usize) -> ! {
    // SAFETY: inherited terminal stack and liveness contract. The tail target
    // never observes the abandoned stack and never returns to this function.
    unsafe {
        core::arch::asm!(
            "mv sp, {stack_top}",
            "andi sp, sp, -16",
            "mv a0, {hart_id}",
            "mv a1, {index}",
            "tail {warm_entry}",
            stack_top = in(reg) stack_top,
            hart_id = in(reg) hart_id,
            index = in(reg) index,
            warm_entry = sym super::warm_entry,
            options(noreturn),
        )
    }
}
