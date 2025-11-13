//! Inter-processor interrupt extension test suite.

use crate::thread::Thread;
use riscv::{
    interrupt::supervisor::{Exception, Interrupt},
    register::{
        scause::{self, Trap},
        sie,
    },
};
use sbi::HartMask;

/// Inter-processor Interrupt extension test cases.
#[derive(Clone, Debug)]
pub enum Case {
    /// Can't proceed test for inter-processor interrupt extension does not exist.
    NotExist,
    /// Test begin.
    Begin,
    /// Test process for an inter-processor interrupt has been received.
    SendIpi,
    /// Test failed for unexpected trap occurred upon tests.
    UnexpectedTrap(Trap<usize, usize>),
    /// All test cases on inter-processor interrupt extension has passed.
    Pass,
}

/// Test inter-processor interrupt extension.
pub fn test(hart_id: usize, mut f: impl FnMut(Case)) {
    if sbi::probe_extension(sbi::Timer).is_unavailable() {
        f(Case::NotExist);
        return;
    }

    fn ipi(hart_id: usize) -> ! {
        sbi::send_ipi(HartMask::from_mask_base(1 << hart_id, 0));
        // 必须立即触发中断，即使是一个指令的延迟，也会触发另一个异常
        unsafe { core::arch::asm!("unimp", options(noreturn, nomem)) };
    }

    f(Case::Begin);
    let mut stack = [0usize; 32];
    let mut thread = Thread::new(ipi as *const () as _);
    *thread.sp_mut() = stack.as_mut_ptr_range().end as _;
    *thread.a_mut(0) = hart_id;
    unsafe {
        sie::set_ssoft();
        thread.execute();
    }
    let trap = scause::read().cause();
    match trap.try_into::<Interrupt, Exception>() {
        Ok(Trap::Interrupt(Interrupt::SupervisorSoft)) => {
            f(Case::SendIpi);
            f(Case::Pass);
        }
        _ => {
            f(Case::UnexpectedTrap(trap));
        }
    }
}
