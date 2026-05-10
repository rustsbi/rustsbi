use crate::cfg::{NUM_HART_MAX, STACK_SIZE_PER_HART};
use crate::riscv::current_hartid;
use crate::sbi::hart_context::HartContext;
use crate::sbi::trap::fast_handler;
use core::cell::UnsafeCell;
use core::mem::{MaybeUninit, forget};
use fast_trap::FreeTrapStack;

/// Root stack array for all harts, placed in BSS Stack section.
///
/// Each entry holds only raw stack bytes; per-hart state lives in the
/// separate `HART_CONTEXTS` array so a stack overflow cannot silently
/// corrupt it.
#[unsafe(link_section = ".bss.stack")]
pub(crate) static mut ROOT_STACK: [Stack; NUM_HART_MAX] = [Stack::ZERO; NUM_HART_MAX];

/// Per-hart contexts, kept outside the trap stacks.
///
/// Storage is zero-initialised in BSS and made fully valid by the
/// owning hart calling `HartContext::init` from `prepare_for_trap`.
static HART_CONTEXTS: [HartContextSlot; NUM_HART_MAX] =
    [const { HartContextSlot::ZEROED }; NUM_HART_MAX];

/// Storage for one HartContext that is zero-initialised in BSS and
/// shareable between harts.
#[repr(transparent)]
struct HartContextSlot(UnsafeCell<MaybeUninit<HartContext>>);

impl HartContextSlot {
    const ZEROED: Self = Self(UnsafeCell::new(MaybeUninit::zeroed()));
}

// Each slot is only mutated by the hart it belongs to, so sharing the
// outer array between harts is sound.
unsafe impl Sync for HartContextSlot {}

// Make sure stack size is a multiple of stack alignment so that each
// hart's stack remains properly aligned in the contiguous array.
const _: () = assert!(STACK_SIZE_PER_HART % core::mem::align_of::<Stack>() == 0);

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

/// Prepares trap stack and HartContext for current hart.
pub(crate) fn prepare_for_trap() {
    let hart_id = current_hartid();
    let hart = hart_context_mut(hart_id);
    let context_ptr = hart.context_ptr();
    hart.init();

    let stack = unsafe { ROOT_STACK.get_unchecked_mut(hart_id) };
    stack.install_canary();
    let range = stack.0.as_ptr_range();

    forget(
        FreeTrapStack::new(
            range.start as usize..range.end as usize,
            |_| {},
            context_ptr,
            fast_handler,
        )
        .unwrap()
        .load(),
    );
}

/// Returns a mutable reference to the HartContext for `hart_id`.
pub fn hart_context_mut(hart_id: usize) -> &'static mut HartContext {
    unsafe { (*HART_CONTEXTS[hart_id].0.get()).assume_init_mut() }
}

/// Returns a shared reference to the HartContext for `hart_id`.
pub fn hart_context(hart_id: usize) -> &'static HartContext {
    unsafe { (*HART_CONTEXTS[hart_id].0.get()).assume_init_ref() }
}

/// Returns the HartContext for `hart_id`, or `None` if out of range.
pub fn try_hart_context(hart_id: usize) -> Option<&'static HartContext> {
    if hart_id >= NUM_HART_MAX {
        return None;
    }
    Some(hart_context(hart_id))
}

/// Magic value written at the bottom of every hart's trap stack so an
/// overflow that wrote past it is detectable on the next check.
const STACK_CANARY: u64 = 0x5BA1_BEEF_DEAD_CA5Fu64;

// Compile-time guarantees: the canary must differ from common
// uninitialised patterns so a stale BSS / wild-pointer write cannot
// pass the canary check by accident.
const _: () = assert!(STACK_CANARY != 0);
const _: () = assert!(STACK_CANARY != u64::MAX);
const _: () = assert!(STACK_CANARY != 0xDEAD_BEEF_DEAD_BEEFu64);

/// Panics with a clear diagnostic if the current hart's trap stack has
/// been overflowed.
///
/// Two complementary checks are performed:
///   * the bottom-of-stack canary, which catches any frame that wrote
///     past the configured stack size; and
///   * `sp` against the stack lower bound, which catches a frame still
///     in the middle of an overflow at the moment of the check.
///
/// Stack overflow used to silently corrupt adjacent BSS state and only
/// surface as a downstream panic in unrelated code; this turns it into
/// an immediate, explainable failure.
#[inline(always)]
pub fn assert_stack_pointer_in_range() {
    let sp: usize;
    unsafe {
        core::arch::asm!("mv {}, sp", out(reg) sp, options(nomem, nostack));
    }
    let hart_id = current_hartid();
    let stack = unsafe { ROOT_STACK.get_unchecked(hart_id) };
    let bottom = stack.0.as_ptr() as usize;
    if sp < bottom {
        panic!(
            "hart {} stack overflow: sp = {:#x} below stack bottom {:#x}",
            hart_id, sp, bottom
        );
    }
    if !stack.canary_intact() {
        panic!(
            "hart {} stack overflow: bottom canary at {:#x} was overwritten",
            hart_id, bottom
        );
    }
}

/// Stack type for each hart.
///
/// Holds raw working memory only; per-hart state lives in the separate
/// `HART_CONTEXTS` array. The first eight bytes hold a canary written
/// in `prepare_for_trap` and checked from trap entry to detect overflow.
#[repr(C, align(128))]
pub(crate) struct Stack([u8; STACK_SIZE_PER_HART]);

impl Stack {
    const ZERO: Self = Self([0; STACK_SIZE_PER_HART]);

    /// Writes the bottom-of-stack canary. Volatile to prevent the
    /// compiler from eliding the store as dead.
    fn install_canary(&mut self) {
        let p = self.0.as_mut_ptr() as *mut u64;
        unsafe { p.write_volatile(STACK_CANARY) };
    }

    /// Returns true if the bottom-of-stack canary still holds the
    /// expected pattern.
    #[inline(always)]
    fn canary_intact(&self) -> bool {
        let p = self.0.as_ptr() as *const u64;
        unsafe { p.read_volatile() == STACK_CANARY }
    }
}
