//! RV64 implementation of thread context switching.
//!
//! Register slots are 8 bytes and the layout follows the contract documented
//! on [`super::Thread`]: `x[n]` lives at `n * 8` relative to the context.

use super::Thread;

/// Executes this thread and returns `sstatus`.
///
/// # Safety
///
/// Modifies `sscratch`, `sepc`, `sstatus` and `stvec`.
#[inline]
pub(super) unsafe fn execute(thread: &mut Thread) -> usize {
    unsafe {
        // Set up the thread to stay in S-mode with interrupts enabled
        let mut sstatus = super::thread_entry_sstatus();
        // Execute the thread. The return address is kept in a 16-byte frame
        // so the stack pointer stays ABI-aligned at the `call`.
        core::arch::asm!(
            "   csrw sscratch, {sscratch}
            csrw sepc    , {sepc}
            csrw sstatus , {sstatus}
            addi sp, sp, -16
            sd   ra, 8(sp)
            call {context_switch}
            ld   ra, 8(sp)
            addi sp, sp,  16
            csrr {sepc}   , sepc
            csrr {sstatus}, sstatus
        ",
            sscratch       = in(reg) thread,
            sepc           = inlateout(reg) thread.sepc,
            sstatus        = inlateout(reg) sstatus,
            context_switch = sym context_switch,
        );
        sstatus
    }
}

/// Context switch core.
///
/// Pushes the general-purpose registers, then restores the thread's
/// general-purpose registers from the context pointer preloaded in
/// `sscratch`.
///
/// # Safety
///
/// Naked function.
#[unsafe(naked)]
unsafe extern "C" fn context_switch() {
    core::arch::naked_asm!(
        r"  .altmacro
        .macro SAVE n
            sd x\n, \n*8(sp)
        .endm
        .macro SAVE_ALL
            sd x1, 1*8(sp)
            .set n, 3
            .rept 29
                SAVE %n
                .set n, n+1
            .endr
        .endm

        .macro LOAD n
            ld x\n, \n*8(sp)
        .endm
        .macro LOAD_ALL
            ld x1, 1*8(sp)
            .set n, 3
            .rept 29
                LOAD %n
                .set n, n+1
            .endr
        .endm
    ",
        // Position-independent loading
        "   .option push
        .option nopic
    ",
        // Save the scheduler context
        "   addi sp, sp, -32*8
        SAVE_ALL
    ",
        // Set up the trap entry
        "   la   t0, 2f
        csrw stvec, t0
    ",
        // Save the scheduler context pointer and switch context
        "   csrr t0, sscratch
        sd   sp, (t0)
        mv   sp, t0
    ",
        // Restore the thread context
        "   LOAD_ALL
        ld   sp, 2*8(sp)
    ",
        // Execute the thread
        "   sret",
        // Trap
        "   .align 2",
        // Switch context
        "2: csrrw sp, sscratch, sp",
        // Save the thread context
        "   SAVE_ALL
        csrrw t0, sscratch, sp
        sd    t0, 2*8(sp)
    ",
        // Switch context
        "   ld sp, (sp)",
        // Restore the scheduler context
        "   LOAD_ALL
        addi sp, sp, 32*8
    ",
        // Return to the scheduler
        "   ret",
        "   .option pop",
    )
}
