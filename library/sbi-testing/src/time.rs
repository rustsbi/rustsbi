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
    /// The timer extension is unavailable.
    NotExist,
    /// The test has begun.
    Begin,
    /// Two successive time counter readings.
    Interval {
        /// The time counter value for the first read.
        begin: u64,
        /// The time counter value for the second read.
        end: u64,
    },
    /// The `time` register could not be read.
    ReadFailed,
    /// The time counter did not increase between two reads.
    TimeDecreased {
        /// The time counter value for the first read.
        a: u64,
        /// The time counter value for the second read.
        b: u64,
    },
    /// The supervisor timer interrupt was received.
    SetTimer,
    /// An unexpected trap occurred during the timer test.
    UnexpectedTrap(Trap<usize, usize>),
    /// The timer test passed.
    Pass,
}

/// Tests the timer extension.
pub fn test(delay: u64, mut f: impl FnMut(Case)) {
    if sbi::probe_extension(sbi::Timer).is_unavailable() {
        f(Case::NotExist);
        return;
    }
    f(Case::Begin);
    let begin: usize;
    let end: usize;
    let mut ok = 0xffusize;
    // SAFETY: The test runs in supervisor mode and sets an aligned trap
    // vector to the continuation before probing the time CSR.
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
    // TODO: Support RV32, where the time counter spans `time` and `timeh`.
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

    // WFI may resume for an interrupt handled in M-mode without an S-mode trap.
    fn wait_for_timer() -> ! {
        loop {
            riscv::asm::wfi();
        }
    }

    let mut stack = [0usize; 32];
    let mut thread = Thread::new(
        wait_for_timer as *const () as _,
        stack.as_mut_ptr_range().end as _,
    );

    sbi::set_timer(time::read64() + delay);
    // SAFETY:
    // 1. The supervisor timer is programmed before its interrupt is enabled.
    // 2. The thread owns its stack and installs a trap entry for returning to this test.
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
