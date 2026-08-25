//! Mechanism module owning the per-hart raw stack array: `ROOT_STACK` is
//! externally-addressed raw memory (the naked trap entry reaches it by
//! `sym ROOT_STACK`), so the slots are `UnsafeCell` storage instead of a
//! `static mut`. Rust code only ever reaches a hart's [`HartLocal`] state
//! through the safe accessors below; references are formed per call and
//! never escape as long-lived `&mut`.

use crate::cfg::{NUM_HART_MAX, STACK_SIZE_PER_HART};
use crate::riscv::current_hartid;
use crate::sbi::hart_context::{HartContext, HartLocal, NextStage};
use crate::sbi::hsm::{LocalHsmCell, RemoteHsmCell};
use crate::sbi::rfence::{LocalRFenceCell, RemoteRFenceCell};
use crate::sbi::trap::fast_handler;
use core::cell::UnsafeCell;
use core::mem::forget;
use core::ptr::{addr_of, addr_of_mut};
use fast_trap::FreeTrapStack;

/// Raw stack slot for each hart.
///
/// Memory layout:
/// - Bottom: HartContext (trap frame + hart-local state).
/// - Middle: Stack space for the hart.
/// - Top: Trap handling space.
#[repr(C, align(128))]
struct HartStack(UnsafeCell<[u8; STACK_SIZE_PER_HART]>);

// SAFETY: `HartStack` is raw storage addressed by hart id: the naked entry
// (`locate`) reaches it via `sym ROOT_STACK` and the trap framework via the
// frame pointer installed by `load_as_stack`. Rust references are only
// formed for the caller's own slot inside `with_current`/`with_hart`, or as
// shared references through `hart_local`; cross-hart sharing goes through
// the atomics and cells stored in `HartLocal`.
unsafe impl Sync for HartStack {}

/// Root stack array for all harts, placed in BSS stack section.
#[used]
#[unsafe(link_section = ".bss.stack")]
static ROOT_STACK: [HartStack; NUM_HART_MAX] = [const { HartStack::zero() }; NUM_HART_MAX];

// Make sure stack address can be aligned.
const _: () = assert!(STACK_SIZE_PER_HART.is_multiple_of(core::mem::align_of::<HartStack>()));

/// Returns the raw slot of `hart_id`, or `None` when out of range.
#[inline]
fn slot(hart_id: usize) -> Option<&'static HartStack> {
    ROOT_STACK.get(hart_id)
}

/// Forms a shared reference to the hart-local state behind a raw slot.
#[inline]
fn local_of(slot: &'static HartStack) -> &'static HartLocal {
    // SAFETY: the slot memory is bounds-checked and static; the resulting
    // shared reference is only used for atomics and interior-mutable cells,
    // matching the pre-split `hart_context()` projection.
    unsafe { &*addr_of!((*slot.0.get().cast::<HartContext>()).local) }
}

/// Locates and initializes stack for each hart.
///
/// This is a naked function that sets up the stack pointer based on hart ID.
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn locate() {
    core::arch::naked_asm!(
        "   la   sp, {stack}            // Load stack base address
            li   t0, {per_hart_stack_size} // Load stack size per hart
            csrr t1, mhartid            // Get current hart ID
            addi t1, t1,  1             // Add 1 to hart ID
         1: add  sp, sp, t0             // Calculate stack pointer
            addi t1, t1, -1             // Decrement counter
            bnez t1, 1b                 // Loop if not zero
            call t1, {move_stack}       // Call stack reuse function
            ret                         // Return
        ",
        per_hart_stack_size = const STACK_SIZE_PER_HART,
        stack               =   sym ROOT_STACK,
        move_stack          =   sym fast_trap::reuse_stack_for_trap,
    )
}

/// Prepares trap stack for current hart
pub(crate) fn prepare_for_trap() {
    // SAFETY: the current hart id always indexes `ROOT_STACK`.
    let slot: &'static HartStack = unsafe { ROOT_STACK.get_unchecked(current_hartid()) };
    HartStack::load_as_stack(slot);
}

/// Runs `f` with exclusive access to the current hart's hart-local state.
///
/// SAFETY (mechanism contract): each slot is owned by exactly one hart, so
/// borrowing that hart's `HartLocal` is exclusive while `f` runs — M-mode
/// trap entry clears MIE and trap/interrupt handlers never call this, so
/// no second borrow can go live. The [`TrapFrame`] part of the slot is
/// unreachable through the given reference.
///
/// [`TrapFrame`]: crate::sbi::hart_context::TrapFrame
pub fn with_current<F, R>(f: F) -> R
where
    F: FnOnce(&mut HartLocal) -> R,
{
    with_hart(current_hartid(), f)
}

/// Runs `f` with exclusive access to an arbitrary hart's hart-local state.
///
/// SAFETY (mechanism contract): the caller must guarantee the target hart
/// does not concurrently touch its own slot while `f` runs (parked in
/// firmware entry, or not yet released). Used by `with_current` and by the
/// boot-time feature seeding in `features::extension_detection`.
pub fn with_hart<F, R>(hart_id: usize, f: F) -> R
where
    F: FnOnce(&mut HartLocal) -> R,
{
    let slot = slot(hart_id).expect("hart id within ROOT_STACK bounds");
    // SAFETY: exclusive to this slot per the contract above; the memory is
    // bounds-checked, static, and disjoint from the trap frame at offset 0.
    let local = unsafe { &mut *addr_of_mut!((*slot.0.get().cast::<HartContext>()).local) };
    f(local)
}

/// Shared projection of any hart's hart-local state.
pub fn hart_local(hart_id: usize) -> &'static HartLocal {
    local_of(slot(hart_id).expect("hart id within ROOT_STACK bounds"))
}

/// Gets the local HSM cell for the current hart.
pub fn local_hsm() -> LocalHsmCell<'static, NextStage> {
    // SAFETY: the cell belongs to the current hart by construction.
    unsafe { hart_local(current_hartid()).hsm.local() }
}

/// Gets a remote view of any hart's HSM cell.
pub fn remote_hsm(hart_id: usize) -> Option<RemoteHsmCell<'static, NextStage>> {
    slot(hart_id).map(local_of).map(|local| local.hsm.remote())
}

/// Gets the local fence context for the current hart.
pub fn local_rfence() -> Option<LocalRFenceCell<'static>> {
    slot(current_hartid())
        .map(local_of)
        .map(|local| local.rfence.local())
}

/// Gets the remote fence context for a specific hart.
pub fn remote_rfence(hart_id: usize) -> Option<RemoteRFenceCell<'static>> {
    slot(hart_id)
        .map(local_of)
        .map(|local| local.rfence.remote())
}

/// Resets the hart-local bookkeeping (ipi type, fence cell, pmu state) of
/// `hart_id`; the trap frame is untouched, as in the pre-split reset.
pub fn reset_hart(hart_id: usize) {
    with_hart(hart_id, HartLocal::reset);
}

impl HartStack {
    /// All-zero slot, usable as an array repeat operand (const fn form;
    /// a `const` item would carry interior mutability).
    const fn zero() -> Self {
        Self(UnsafeCell::new([0; STACK_SIZE_PER_HART]))
    }

    /// Initializes stack for trap handling.
    /// - Sets up hart context.
    /// - Creates and loads FreeTrapStack with the stack range.
    fn load_as_stack(slot: &'static Self) {
        let context = slot.0.get().cast::<HartContext>();
        // SAFETY: this hart owns `slot`, and the trap entry starts using the
        // installed frame pointer only once `FreeTrapStack::load` runs below.
        let context_ptr = unsafe { (*addr_of_mut!((*context).frame)).context_ptr() };
        unsafe { (*addr_of_mut!((*context).local)).init() };

        // Get stack memory range.
        let range = unsafe { (*slot.0.get()).as_ptr_range() };

        // Create and load trap stack, forgetting it to avoid drop
        forget(
            FreeTrapStack::new(
                range.start as usize..range.end as usize,
                |_| {}, // Empty callback
                context_ptr,
                fast_handler,
            )
            .unwrap()
            .load(),
        );
    }
}
