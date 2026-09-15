//! The firmware-entry adapter: the boot-domain pieces the architectural
//! entry path connects to.
//!
//! Stack selection and allocation belong to boot and are referenced by
//! the entry assembly. This module owns
//!
//! - [`fail_stop`]: the stack-independent early and fatal vector;
//! - [`locate_stack`]: per-hart Runtime stack selection with the hart bound
//!   checked before any address arithmetic;
//! - [`finish_boot`]: the never-returning end of boot — it discards the
//!   boot call chain, arms the reused stack as the trap stack, and either
//!   `mret`s into the staged S/HS next stage or parks the hart until one is
//!   staged.
//!
//! The procedural entry macro in `firmware/macros` references these symbols
//! directly; they are not policy API.

mod stack;

pub use stack::locate_stack;

use crate::hart::{self, HartEvent};
use crate::trap::init::mark_armed;

/// Information needed to boot into the next execution stage.
#[derive(Clone, Copy, Debug)]
pub struct NextStage {
    /// Starting address to jump to.
    pub start_addr: usize,
    /// Opaque value passed to next stage.
    pub opaque: usize,
    /// Privilege mode for next stage.
    pub next_mode: riscv::register::mstatus::MPP,
}

/// The stack-independent fail-stop vector: the early `mtvec` target
/// installed at firmware entry before any Runtime state exists, and the
/// terminal target of every fatal path.
///
/// # Safety
///
/// Naked `mtvec` target, never a callable function.
#[unsafe(naked)]
#[unsafe(export_name = "runtime_fail_stop")]
pub unsafe extern "C" fn fail_stop() {
    core::arch::naked_asm!(".align 2", "csrw mie, zero", "1: wfi", "   j 1b",);
}

/// Enters a K1 hart released from hardware reset, without an SPL handoff.
///
/// # Safety
///
/// Assembly entry on a K1 hart only. The boot hart must have published the
/// platform and enabled cluster coherency before releasing this hart.
/// `initialize` performs that hart's safe platform setup and trap activation.
#[unsafe(naked)]
pub unsafe extern "C" fn k1_warm_entry(initialize: extern "C" fn()) -> ! {
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
        "jalr s0",
        "tail {finish}",
        fail = sym fail_stop,
        prepare = sym crate::SpacemitK1Registers::prepare_warm_hart,
        locate = sym locate_stack,
        finish = sym finish_boot,
    )
}

/// Enters a V861 C907 hart released from hardware reset.
///
/// # Safety
///
/// Only V861 hardware reset may enter here, after shared Runtime and platform
/// state is published. `initialize` restores cache policy and activates traps.
#[unsafe(naked)]
pub unsafe extern "C" fn v861_warm_entry(initialize: extern "C" fn()) -> ! {
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
        "jalr s0",
        "tail {finish}",
        fail = sym fail_stop,
        locate = sym locate_stack,
        finish = sym finish_boot,
    )
}

/// Enters the next stage or parks after boot or a hart stop. Never returns.
///
/// One stack per hart is sequentially reused for boot and traps, so before
/// that stack may serve traps, the still-live boot or stopped trap call chain must be
/// discarded: `sp` is reset to the clean stack top, the top is published in
/// `mscratch` (the Armed condition), and the hart then either executes the
/// final `mret` into the staged S/HS next stage or parks until one is
/// staged. Initial boot establishes Ready → Armed here; stopping a hart
/// reuses this path to discard its old supervisor context.
///
/// # Safety
///
/// The caller must run in M-mode with interrupts disabled. The current
/// boot or stopped trap call chain must be safe to discard without unwinding.
#[unsafe(naked)]
#[unsafe(export_name = "runtime_finish_boot")]
pub unsafe extern "C" fn finish_boot() -> ! {
    core::arch::naked_asm!(
        ".align 2",
        // Discard the current call chain: sp becomes this hart's clean top.
        "call {locate}",
        // Arm the trap stack: from here, a lower-origin trap enters through
        // the clean top, and the parked wait runs on a valid Runtime stack.
        "csrw mscratch, sp",
        "call {finisher}",
        // The finisher diverges; reaching this point is a firmware bug.
        "j {fail_stop}",
        locate = sym locate_stack,
        finisher = sym finish_boot_rust,
        fail_stop = sym fail_stop,
    );
}

/// The Rust side of the boot finisher: enter the staged next stage, or park
/// until one arrives.
fn finish_boot_rust() -> ! {
    let hart = hart::current_hart();
    mark_armed(hart.as_usize());
    loop {
        // Acknowledge before observing the state. Clearing after observing
        // Stopped could erase a concurrent start's wakeup just before WFI.
        if let Some(ipi) = crate::ipi::get() {
            ipi.clear_current()
                .expect("BUG: could not clear the hart wake interrupt");
            riscv::asm::fence();
        }
        let event = hart::take_local_event();
        // A sender may have selected this hart before it stopped, or just
        // after STARTED became visible. Complete its work before handoff/WFI.
        if let Some(handler) = crate::ipi::handler() {
            handler.deliver_current();
        }
        match event {
            HartEvent::Start(next_stage) => {
                // SAFETY: M-mode writes to this hart's S-mode and trap
                // CSRs for the staged entry.
                unsafe {
                    crate::trap::dispatch::stage_next_mode(
                        next_stage.start_addr,
                        next_stage.next_mode,
                    )
                };
                let hart_id = hart.as_usize();
                // SAFETY: the diverging final `mret` into S/HS mode; the
                // ceremony above staged every CSR the architecture needs.
                unsafe { enter_stage(hart_id, next_stage.opaque) }
            }
            HartEvent::Park => {
                // SAFETY: M-mode write to this hart's mie around the parked
                // wait; a staged start wakes this hart into the machine
                // software transport, which performs the entry.
                crate::csr::mie::set_machine_software();
                riscv::asm::wfi();
                // A masked wake re-checks the cell instead of returning
                // into the discarded boot context.
                continue;
            }
            // A fresh cell is STOPPED or START_PENDING at this point; any
            // other state is a firmware bug.
            HartEvent::None => unreachable!("boot-stage hart is neither start nor park"),
        }
    }
}

/// Hands off with only the SBI entry arguments retained in general registers.
///
/// # Safety
/// The next-stage CSRs and Runtime trap stack must already be prepared.
#[unsafe(naked)]
unsafe extern "C" fn enter_stage(hart_id: usize, opaque: usize) -> ! {
    core::arch::naked_asm!(
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
    )
}
