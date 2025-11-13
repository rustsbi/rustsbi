//! Timer programmer extension test suite.

use crate::thread::Thread;
use riscv::{
    interrupt::supervisor::{Exception, Interrupt},
    register::{
        scause::{self, Trap},
        sie, time,
    },
};

/// Timer programmer extension test cases.
#[derive(Clone, Debug)]
pub enum Case {
    /// Can't proceed test for Timer extension does not exist.
    NotExist,
    /// Test begin.
    Begin,
    /// Test process for time interval overhead between two reads.
    Interval {
        /// The time counter value for the first read.
        begin: u64,
        /// The time counter value for the second read.
        end: u64,
    },
    /// Test failed for can't read `time` register.
    ReadFailed,
    /// Test failed for time counter has decreased during period of two reads.
    TimeDecreased {
        /// The time counter value for the first read.
        a: u64,
        /// The time counter value for the second read.
        b: u64,
    },
    /// Test process for timer has been set.
    SetTimer,
    /// Test failed for unexpected trap during timer test.
    UnexpectedTrap(Trap<usize, usize>),
    /// All test cases on timer extension has passed.
    Pass,
}

/// Test timer extension.
pub fn test(delay: u64, mut f: impl FnMut(Case)) {
    if sbi::probe_extension(sbi::Timer).is_unavailable() {
        f(Case::NotExist);
        return;
    }
    f(Case::Begin);
    let begin: usize;
    let end: usize;
    let mut ok = 0xffusize;
    unsafe {
        core::arch::asm!(
            "   la   {stvec}, 2f
                csrw stvec,   {stvec}
                csrr {begin}, time
                csrr {end},   time
                mv   {ok},    zero
            .align 2
            2:
            ",
            stvec = out(reg) _,
            begin = out(reg) begin,
            end   = out(reg) end,
            ok    = inlateout(reg) ok,
        );
    }
    // TODO: support RV32 where there are time and timeh registers.
    let begin: u64 = begin as u64;
    let end: u64 = end as u64;
    if ok != 0 {
        f(Case::ReadFailed);
        return;
    }
    if begin >= end {
        f(Case::TimeDecreased { a: begin, b: end });
        return;
    }
    f(Case::Interval { begin, end });

    let mut stack = [0usize; 32];
    let mut thread = Thread::new(riscv::asm::wfi as *const () as _);
    *thread.sp_mut() = stack.as_mut_ptr_range().end as _;

    sbi::set_timer(time::read64() + delay);
    unsafe {
        sie::set_stimer();
        thread.execute();
    }
    let trap = scause::read().cause();
    match trap.try_into::<Interrupt, Exception>() {
        Ok(Trap::Interrupt(Interrupt::SupervisorTimer)) => {
            sbi::set_timer(u64::MAX);
            f(Case::SetTimer);
            f(Case::Pass);
        }
        _ => {
            f(Case::UnexpectedTrap(trap));
        }
    }
}
