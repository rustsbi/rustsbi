//! Runtime-owned per-hart stacks.
//!
//! One fixed-size region per supported hart, placed by the firmware linker
//! script in `.bss.stack`. Each slot is sequentially reused: boot Rust runs
//! on it after entry, `finish_boot` discards the boot call chain, and every
//! later trap starts from the clean top through `mscratch`.

use core::cell::UnsafeCell;

use crate::cfg::{NUM_HART_MAX, STACK_SIZE_PER_HART};

/// Raw stack slot for each hart.
#[repr(C, align(128))]
struct HartStack(UnsafeCell<[u8; STACK_SIZE_PER_HART]>);

// SAFETY: `HartStack` is raw storage addressed by hart id: the naked entry
// reaches it via `sym ROOT_STACK`, and Rust code only forms references to
// the caller's own slot through checked accessors.
unsafe impl Sync for HartStack {}

/// Root stack array for all harts, placed in the BSS stack section.
#[used]
#[unsafe(link_section = ".bss.stack")]
static ROOT_STACK: [HartStack; NUM_HART_MAX] = [const { HartStack::zero() }; NUM_HART_MAX];

// Slots are pairwise disjoint by construction and every Rust call boundary
// stays 16-byte aligned: slot starts are stack-size aligned and the size is
// a multiple of 16.
const _: () = assert!(STACK_SIZE_PER_HART.is_multiple_of(core::mem::align_of::<HartStack>()));
const _: () = assert!(STACK_SIZE_PER_HART.is_multiple_of(16));

impl HartStack {
    /// All-zero slot, usable as an array repeat operand.
    const fn zero() -> Self {
        Self(UnsafeCell::new([0; STACK_SIZE_PER_HART]))
    }
}

/// Locates the current hart's stack and moves `sp` to its clean top.
///
/// The bound is validated before any address arithmetic: a hart beyond the
/// configured capacity parks on the stack-independent fail-stop path.
///
/// # Safety
///
/// Naked helper for the firmware entry and `finish_boot`; it must
/// run before Rust relies on `sp`.
#[unsafe(naked)]
pub unsafe extern "C" fn locate_stack() {
    core::arch::naked_asm!(
        "   csrr  t1, mhartid            // Get current hart ID
            li    t0, {num_hart_max}
            bgeu  t1, t0, 2f            // Out of range: fail-stop
            la    sp, {stack}            // Load stack base address
            li    t0, {per_hart_stack_size} // Load stack size per hart
            addi  t1, t1,  1             // Add 1 to hart ID
         1: add   sp, sp, t0             // Calculate stack pointer
            addi  t1, t1, -1             // Decrement counter
            bnez  t1, 1b                 // Loop if not zero
            ret                         // sp = this hart's clean stack top
         2: j     {fail_stop}
        ",
        per_hart_stack_size = const STACK_SIZE_PER_HART,
        num_hart_max = const NUM_HART_MAX,
        stack = sym ROOT_STACK,
        fail_stop = sym super::fail_stop,
    )
}
