//! Thread context and minimal context switching.
//!
//! The SBI test suites need to run a short code snippet and catch the first
//! interrupt or exception it triggers (for example, "did the timer interrupt
//! actually fire?"). [`Thread`] provides exactly that: a one-shot execution
//! context that switches away from the caller, runs the target function, and
//! switches back on the first trap.
//!
//! The architecture-specific switchers live in `rv64.rs` and `rv32.rs`. They
//! share the same protocol and differ only in register-slot width and the
//! `sd`/`ld` vs `sw`/`lw` instructions.

use core::mem::offset_of;

#[cfg(target_pointer_width = "64")]
mod rv64;
#[cfg(target_pointer_width = "64")]
use rv64 as imp;
#[cfg(target_pointer_width = "32")]
mod rv32;
#[cfg(target_pointer_width = "32")]
use rv32 as imp;

/// Thread context.
///
/// The memory layout is part of the contract with the context-switch
/// assembly; the assertions below pin it at compile time. With `W` being
/// the register width in bytes (8 on RV64, 4 on RV32):
///
/// ```text
/// offset 0     sctx   scheduler stack pointer, saved while the thread runs
/// offset 1*W   x[0]   x1 (ra)
/// offset 2*W   x[1]   x2 (sp)
/// ...          ...
/// offset 31*W  x[30]  x31
/// offset 32*W  sepc   thread entry / resume address
/// ```
#[repr(C)]
pub struct Thread {
    sctx: usize,
    x: [usize; 31],
    sepc: usize,
}

// The switch assembly addresses x[n] as `n * size_of::<usize>(sp)` with `sp`
// pointing at `sctx`; keep the layout pinned so a field reorder fails at
// compile time instead of silently restoring the wrong registers.
const _: () = assert!(offset_of!(Thread, sctx) == 0);
const _: () = assert!(offset_of!(Thread, x) == core::mem::size_of::<usize>());
const _: () = assert!(offset_of!(Thread, sepc) == 32 * core::mem::size_of::<usize>());
const _: () = assert!(core::mem::size_of::<Thread>() == 33 * core::mem::size_of::<usize>());
const _: () = assert!(core::mem::align_of::<Thread>() == core::mem::size_of::<usize>());

#[allow(unused)]
impl Thread {
    /// Creates a blank context entering `entry` with stack pointer `sp`.
    ///
    /// All general-purpose registers start as zero except the stack pointer;
    /// use [`Thread::a_mut`] to set argument registers before execution.
    #[inline]
    pub const fn new(entry: usize, sp: usize) -> Self {
        let mut this = Self {
            sctx: 0,
            x: [0; 31],
            sepc: entry,
        };
        this.x[1] = sp;
        this
    }

    /// Reads a general-purpose register.
    ///
    /// `n` must be in `1..=31`.
    #[inline]
    pub fn x(&self, n: usize) -> usize {
        debug_assert!((1..=31).contains(&n));
        self.x[n - 1]
    }

    /// Mutates a general-purpose register.
    ///
    /// `n` must be in `1..=31`.
    #[inline]
    pub fn x_mut(&mut self, n: usize) -> &mut usize {
        debug_assert!((1..=31).contains(&n));
        &mut self.x[n - 1]
    }

    /// Reads an argument register (`a0` is `n == 0`).
    #[inline]
    pub fn a(&self, n: usize) -> usize {
        self.x(n + 10)
    }

    /// Mutates an argument register (`a0` is `n == 0`).
    #[inline]
    pub fn a_mut(&mut self, n: usize) -> &mut usize {
        self.x_mut(n + 10)
    }

    /// Reads the stack pointer.
    #[inline]
    pub fn sp(&self) -> usize {
        self.x(2)
    }

    /// Mutates the stack pointer.
    #[inline]
    pub fn sp_mut(&mut self) -> &mut usize {
        self.x_mut(2)
    }

    /// Moves pc to the next instruction.
    ///
    /// # Notice
    ///
    /// Assumes the current instruction is not a compressed one.
    #[inline]
    pub fn move_next(&mut self) {
        self.sepc = self.sepc.wrapping_add(4);
    }

    /// Executes this thread and returns `sstatus`.
    ///
    /// Runs the entry function until the first trap (interrupt or exception),
    /// then switches back to the caller. After the return, `scause` tells
    /// which trap arrived and this context holds the thread's last state.
    ///
    /// # Safety
    ///
    /// Modifies `sscratch`, `sepc`, `sstatus` and `stvec`.
    #[inline]
    pub unsafe fn execute(&mut self) -> usize {
        unsafe { imp::execute(self) }
    }
}

/// Reads `sstatus` and sets the thread-entry flags: stay in S-mode with
/// interrupts enabled.
#[inline]
fn thread_entry_sstatus() -> usize {
    let mut sstatus: usize;
    unsafe {
        core::arch::asm!("csrr {}, sstatus", out(reg) sstatus);
    }
    const PRIVILEGE_BIT: usize = 1 << 8;
    const INTERRUPT_BIT: usize = 1 << 5;
    sstatus | PRIVILEGE_BIT | INTERRUPT_BIT
}
