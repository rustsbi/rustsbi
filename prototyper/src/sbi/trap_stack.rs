use core::mem::forget;
use fast_trap::FreeTrapStack;

use crate::sbi::hart_context::HartContext;
use crate::sbi::trap::fast_handler;
use crate::riscv_spec::current_hartid;

const LEN_STACK_PER_HART: usize = 16 * 1024;
pub const NUM_HART_MAX: usize = 8;
/// 栈空间。
#[link_section = ".bss.uninit"]
pub(crate) static mut ROOT_STACK: [Stack; NUM_HART_MAX] = [Stack::ZERO; NUM_HART_MAX];

/// 定位每个 hart 的栈。
#[naked]
pub(crate) unsafe extern "C" fn locate() {
    core::arch::asm!(
        "   la   sp, {stack}
            li   t0, {per_hart_stack_size}
            csrr t1, mhartid
            addi t1, t1,  1
         1: add  sp, sp, t0
            addi t1, t1, -1
            bnez t1, 1b
            call t1, {move_stack}
            ret
        ",
        per_hart_stack_size = const LEN_STACK_PER_HART,
        stack               =   sym ROOT_STACK,
        move_stack          =   sym fast_trap::reuse_stack_for_trap,
        options(noreturn),
    )
}

/// 预备陷入栈。
pub(crate) fn prepare_for_trap() {
    unsafe { ROOT_STACK.get_unchecked_mut(current_hartid()).load_as_stack() };
}

/// 类型化栈。
///
/// 每个硬件线程拥有一个满足这样条件的内存块。
/// 这个内存块的底部放着硬件线程状态 [`HartContext`]，顶部用于陷入处理，中间是这个硬件线程的栈空间。
/// 不需要 M 态线程，每个硬件线程只有这一个栈。
#[repr(C, align(128))]
pub(crate) struct Stack([u8; LEN_STACK_PER_HART]);

impl Stack {
    /// 零初始化以避免加载。
    const ZERO: Self = Self([0; LEN_STACK_PER_HART]);

    /// 从栈上取出硬件线程状态。
    #[inline]
    pub fn hart_context(&mut self) -> &mut HartContext {
        unsafe { &mut *self.0.as_mut_ptr().cast() }
    }

    fn load_as_stack(&'static mut self) {
        let hart = self.hart_context();
        let context_ptr = hart.context_ptr();
        hart.init();
        let range = self.0.as_ptr_range();
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
}
