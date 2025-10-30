use crate::cfg::{NUM_HART_MAX, STACK_SIZE_PER_HART};
use crate::riscv::current_hartid;
use crate::sbi::hart_context::HartContext;
use crate::sbi::trap::fast_handler;
use core::mem::forget;
use fast_trap::FreeTrapStack;

/// Root stack array for all harts, placed in BSS Stack section.
#[unsafe(link_section = ".bss.stack")]
pub(crate) static mut ROOT_STACK: [Stack; NUM_HART_MAX] = [Stack::ZERO; NUM_HART_MAX];

// Make sure stack address can be aligned.
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

/// Prepares trap stack for current hart
pub(crate) fn prepare_for_trap() {
    unsafe {
        ROOT_STACK
            .get_unchecked_mut(current_hartid())
            .load_as_stack()
    };
}

pub fn hart_context_mut(hart_id: usize) -> &'static mut HartContext {
    unsafe { ROOT_STACK.get_mut(hart_id).unwrap().hart_context_mut() }
}

pub fn hart_context(hart_id: usize) -> &'static HartContext {
    unsafe { ROOT_STACK.get(hart_id).unwrap().hart_context() }
}

/// Stack type for each hart.
///
/// Memory layout:
/// - Bottom: HartContext struct.
/// - Middle: Stack space for the hart.
/// - Top: Trap handling space.
///
/// Each hart has a single stack that contains both its context and working space.
#[repr(C, align(128))]
pub(crate) struct Stack([u8; STACK_SIZE_PER_HART]);

impl Stack {
    const ZERO: Self = Self([0; STACK_SIZE_PER_HART]);

    /// Gets mutable reference to hart context at bottom of stack.
    #[inline]
    pub fn hart_context_mut(&mut self) -> &mut HartContext {
        unsafe { &mut *self.0.as_mut_ptr().cast() }
    }

    /// Gets immutable reference to hart context at bottom of stack.
    #[inline]
    pub fn hart_context(&self) -> &HartContext {
        unsafe { &*self.0.as_ptr().cast() }
    }

    /// Initializes stack for trap handling.
    /// - Sets up hart context.
    /// - Creates and loads FreeTrapStack with the stack range.
    fn load_as_stack(&'static mut self) {
        let hart = self.hart_context_mut();
        let context_ptr = hart.context_ptr();
        hart.init();

        // Get stack memory range.
        let range = self.0.as_ptr_range();

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
