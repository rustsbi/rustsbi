//! Firmware startup, hart stacks, and initial privilege transitions.
//!
//! Runtime's [`entry`](crate::entry) macro uses the startup and stack helpers.
//! Trap handling shares the supervisor-context setup for hart starts and
//! resumes.

mod context;
mod environment;
mod stack;
mod stage;
mod thead;

pub(crate) use context::configure_next_stage;
pub use context::prepare_traps;
pub(crate) use context::{prepare_supervisor, set_return_mode, traps_ready};
pub use stage::NextStage;

pub(crate) use stack::firmware_end;
pub use stack::{BootStack, initialize_stacks, locate_stack};

use crate::hart::Hart;

/// Stops the current hart without accessing its stack.
///
/// # Safety
///
/// Use only as an M-mode trap vector or assembly jump target.
/// Control does not return to the caller.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
#[unsafe(export_name = "runtime_fail_stop")]
pub unsafe extern "C" fn fail_stop() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(".balign 4", "csrw mie, zero", "1: wfi", "   j 1b",);
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("The fail-stop vector requires a RISC-V target");
}

/// Enters a K1 hart released from hardware reset, without an SPL handoff.
///
/// # Safety
///
/// Assembly entry on a K1 hart only. The boot hart must have published the
/// platform and enabled cluster coherency before releasing this hart.
/// `initialize` performs per-hart platform setup and publishes SBI policy.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
pub unsafe extern "C" fn k1_warm_entry(initialize: extern "C" fn()) -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(
        ".balign 4",
        "csrw mie, zero",
        "csrci mstatus, 8",
        "la t0, {fail}",
        "csrw mtvec, t0",
        "csrw mscratch, zero",
        "mv s0, a0",
        "call {prepare}",
        "call {locate}",
        "call {prepare_traps}",
        "jalr s0",
        "tail {finish}",
        fail = sym fail_stop,
        prepare = sym crate::SpacemitK1Registers::prepare_warm_hart,
        locate = sym locate_stack,
        prepare_traps = sym prepare_traps,
        finish = sym finish_boot,
    );
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = initialize;
        unimplemented!("K1 warm entry requires a RISC-V target");
    }
}

/// Enters a C907 hart released from hardware reset.
///
/// # Safety
///
/// Only a compatible C907 reset controller may enter here, after shared
/// Runtime and platform state are published. `initialize` restores the
/// platform-owned cache policy and publishes SBI policy.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
pub unsafe extern "C" fn c907_reset_entry(initialize: extern "C" fn()) -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(
        ".balign 4",
        "csrw mie, zero",
        "csrci mstatus, 8",
        "la t0, {fail}",
        "csrw mtvec, t0",
        "csrw mscratch, zero",
        "mv s0, a0",
        // Invalidate caches and join coherency before using shared memory.
        "li t0, 0x70013",
        "csrw 0x7c2, t0",
        "li t0, 1",
        "csrw 0x7f3, t0",
        "fence rw, rw",
        "call {locate}",
        "call {prepare_traps}",
        "jalr s0",
        "tail {finish}",
        fail = sym fail_stop,
        locate = sym locate_stack,
        prepare_traps = sym prepare_traps,
        finish = sym finish_boot,
    );
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = initialize;
        unimplemented!("C907 reset entry requires a RISC-V target");
    }
}

/// Enters the next stage or parks after boot or a hart stop.
///
/// Discards the current call chain because boot and traps reuse one hart stack.
/// This function does not return.
///
/// # Safety
///
/// Run in M-mode with interrupts disabled and an initialized hart stack.
/// The current call chain must be safe to discard without unwinding.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
#[unsafe(export_name = "runtime_finish_boot")]
pub unsafe extern "C" fn finish_boot() -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(
        ".balign 4",
        // Discard the current call chain: sp becomes this hart's clean top.
        "call {locate}",
        // Runtime Rust uses the interrupted M stack for machine exceptions.
        "csrw mscratch, zero",
        "call {finisher}",
        // The finisher diverges; reaching this point is a firmware bug.
        "j {fail_stop}",
        locate = sym locate_stack,
        finisher = sym finish_boot_rust,
        fail_stop = sym fail_stop,
    );
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = finish_boot_rust;
        unimplemented!("Boot completion requires a RISC-V target");
    }
}

/// Enters a staged next stage or waits for a hart-start event.
fn finish_boot_rust() -> ! {
    context::complete_setup();
    // SAFETY: The boot finisher is entered with MIE clear and never enables it.
    let irq = unsafe { crate::irq::LocalIrqDisabled::assume_disabled() };
    let hart = Hart::current().expect("boot hart");
    let next_stage = hart.wait_for_start(&irq);
    // SAFETY: The boot finisher owns the outgoing M-mode context with MIE clear.
    unsafe { context::stage_next_mode(next_stage.start_addr(), next_stage.next_mode()) };
    // SAFETY: The next-stage CSRs and the initialized trap stack are ready.
    unsafe { enter_stage(hart.id().as_usize(), next_stage.opaque()) }
}

/// Hands off with only the SBI entry arguments retained in general registers.
///
/// # Safety
///
/// Run in M-mode with interrupts disabled.
/// The next-stage CSRs and Runtime trap stack must already be prepared.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
unsafe extern "C" fn enter_stage(hart_id: usize, opaque: usize) -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(
        // This function diverges. Retire the finisher's Rust stack before
        // publishing the clean trap stack for the next-stage payload.
        "call {locate}",
        "csrw mscratch, sp",
        "fence.i",
        "li ra, 0",
        "li sp, 0",
        "li gp, 0",
        "li tp, 0",
        "li t0, 0",
        "li t1, 0",
        "li t2, 0",
        "li s0, 0",
        "li s1, 0",
        "li a2, 0",
        "li a3, 0",
        "li a4, 0",
        "li a5, 0",
        "li a6, 0",
        "li a7, 0",
        "li s2, 0",
        "li s3, 0",
        "li s4, 0",
        "li s5, 0",
        "li s6, 0",
        "li s7, 0",
        "li s8, 0",
        "li s9, 0",
        "li s10, 0",
        "li s11, 0",
        "li t3, 0",
        "li t4, 0",
        "li t5, 0",
        "li t6, 0",
        "mret",
        locate = sym locate_stack,
    );
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = (hart_id, opaque);
        unimplemented!("Next-stage entry requires a RISC-V target");
    }
}
