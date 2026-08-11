//! RV32 implementation of thread context switching.
//!
//! Register slots are 4 bytes and the layout follows the contract documented
//! on [`super::Thread`]: `x[n]` lives at `n * 4` relative to the context.

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
        // Execute the thread; 4-byte register slots, `sw`/`lw` instead of
        // `sd`/`ld`. The return address is kept in a 16-byte frame so the
        // stack pointer stays ABI-aligned at the `call`.
        core::arch::asm!(
            "   csrw sscratch, {sscratch}
            csrw sepc    , {sepc}
            csrw sstatus , {sstatus}
            addi sp, sp, -16
            sw   ra, 8(sp)
            call {context_switch}
            lw   ra, 8(sp)
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
/// Same control flow as the RV64 variant, with 4-byte register slots and
/// `sw`/`lw` instead of `sd`/`ld`.
///
/// # Safety
///
/// Naked function.
#[unsafe(naked)]
unsafe extern "C" fn context_switch() {
    core::arch::naked_asm!(
        r"  .altmacro
        .macro SAVE n
            sw x\n, \n*4(sp)
        .endm
        .macro SAVE_ALL
            sw x1, 1*4(sp)
            .set n, 3
            .rept 29
                SAVE %n
                .set n, n+1
            .endr
        .endm

        .macro LOAD n
            lw x\n, \n*4(sp)
        .endm
        .macro LOAD_ALL
            lw x1, 1*4(sp)
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
        "   addi sp, sp, -32*4
        SAVE_ALL
    ",
        // Set up the trap entry
        "   la   t0, 2f
        csrw stvec, t0
    ",
        // Save the scheduler context pointer and switch context
        "   csrr t0, sscratch
        sw   sp, (t0)
        mv   sp, t0
    ",
        // Restore the thread context
        "   LOAD_ALL
        lw   sp, 2*4(sp)
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
        sw    t0, 2*4(sp)
    ",
        // Switch context
        "   lw sp, (sp)",
        // Restore the scheduler context
        "   LOAD_ALL
        addi sp, sp, 32*4
    ",
        // Return to the scheduler
        "   ret",
        "   .option pop",
    )
}
