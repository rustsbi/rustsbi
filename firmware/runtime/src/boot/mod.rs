//! Firmware startup, hart stacks, and initial privilege transitions.
//!
//! Runtime's [`entry`](crate::entry) macro uses the startup and stack helpers.
//! Trap handling shares the supervisor-context setup for hart starts and
//! resumes.

mod context;
mod stack;

pub use crate::pmu::inhibit_programmable as inhibit_programmable_counters;

pub use context::{
    configure_next_stage, configure_state_access, configure_supervisor_envcfg,
    enable_supervisor_seed, prepare_traps,
};
pub(crate) use context::{prepare_supervisor, set_return_mode};

pub(crate) use stack::linked_image_range;
pub use stack::{BootStack, initialize_stacks, locate_firmware_image, locate_stack};

use crate::{
    hart::{self, HartEvent},
    instruction::{self, ALL},
    ipi,
    irq::LocalIrqDisabled,
};
use csr::{
    Clear, Read, Write,
    standard::isa::{
        privileged::machine::{Identifier, Mvendorid},
        values::status::Privilege,
    },
    vendor::thead::Mxstatus,
};

/// Disables non-standard page memory attributes on a T-Head hart.
///
/// Skips the update on other vendors, when trap recovery is unavailable, or
/// when the initial read of `mxstatus` traps.
///
/// # Safety
///
/// Execute during M-mode startup, exclusively own this hart's translation
/// setup, and ensure no lower-mode address space is active. On T-Head harts
/// with a readable `mxstatus`, the implementation must support clearing MAEE.
pub unsafe fn disable_thead_maee() {
    const THEAD_VENDOR_ID: Identifier = Identifier::from_bits(0x5b7);
    if !crate::recovery::is_ready() || Mvendorid::read() != THEAD_VENDOR_ID {
        return;
    }
    crate::irq::with_local_disabled(|_| {
        // SAFETY: Runtime recovery is ready with MPRV/mscratch clear, local
        // interrupts are excluded, and startup owns translation configuration.
        // The caller confirms this implementation's MXSTATUS/MAEE semantics.
        unsafe {
            if Mxstatus::try_read().is_ok() {
                Mxstatus::write(Clear(Mxstatus::MAEE));
            }
        }
    });
}

/// Information needed to boot into the next execution stage.
#[derive(Clone, Copy, Debug)]
pub struct NextStage {
    /// Starting address to jump to.
    pub start_addr: usize,
    /// Opaque value passed to next stage.
    pub opaque: usize,
    /// Privilege mode for next stage.
    pub next_mode: Privilege,
}

/// Stops the current hart without accessing its stack.
///
/// # Safety
///
/// Use only as an M-mode trap vector or assembly jump target.
/// Control does not return to the caller.
#[unsafe(naked)]
#[unsafe(export_name = "runtime_fail_stop")]
pub unsafe extern "C" fn fail_stop() {
    core::arch::naked_asm!(".align 2", "csrw mie, zero", "1: wfi", "   j 1b",);
}

/// Starts a hart released from hardware reset without a bootloader handoff.
///
/// # Safety
///
/// Enter in M-mode with MPRV clear after the boot hart publishes topology and
/// platform state. `prepare` must make shared memory accessible using only
/// instructions and registers. It must return with machine interrupts masked
/// and preserve the C ABI's callee-saved and fixed registers.
/// `initialize` runs with Runtime traps installed and must finish per-hart
/// configuration before returning.
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn reset_hart_entry(
    prepare: unsafe extern "C" fn(),
    initialize: unsafe extern "C" fn(),
) -> ! {
    core::arch::naked_asm!(
        ".balign 4",
        "csrw mie, zero",
        "csrci mstatus, 8",
        ".option push",
        ".option norelax",
        "lla t0, {fail}",
        ".option pop",
        "csrw mtvec, t0",
        "csrw mscratch, zero",
        "mv s0, a1",
        // The platform must prepare coherency before locate_stack reads the hart table.
        "jalr a0",
        "call {locate}",
        "call {prepare_traps}",
        "jalr s0",
        "tail {finish}",
        fail = sym fail_stop,
        locate = sym locate_stack,
        prepare_traps = sym prepare_traps,
        finish = sym finish_boot,
    );
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
#[unsafe(naked)]
#[unsafe(export_name = "runtime_finish_boot")]
pub unsafe extern "C" fn finish_boot() -> ! {
    core::arch::naked_asm!(
        ".align 2",
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
}

/// Enters a staged next stage or waits for a hart-start event.
fn finish_boot_rust() -> ! {
    // SAFETY: The boot finisher is entered with MIE clear and never enables it.
    let irq = unsafe { LocalIrqDisabled::assume_disabled() };
    let hart = hart::current_hart();
    let ipi = ipi::get();
    loop {
        // Acknowledge before observing the state. Clearing after observing
        // Stopped could erase a concurrent start's wakeup just before WFI.
        if let Some(ipi) = ipi {
            ipi.acknowledge(&irq)
                .expect("BUG: could not acknowledge the hart wake interrupt");
            instruction::fence(ALL, ALL);
        }
        let event = hart::take_local_event();
        // A sender may have selected this hart before it stopped, or just
        // after STARTED became visible. Complete its work before handoff/WFI.
        if let Some(ipi) = ipi {
            ipi.process_pending(&irq);
        }
        match event {
            HartEvent::Start(next_stage) => {
                // SAFETY: The boot finisher owns the outgoing context in
                // M-mode, with interrupts disabled and Runtime entry installed.
                unsafe { context::stage_next_mode(next_stage.start_addr, next_stage.next_mode) };
                let hart_id = hart.as_usize();
                // SAFETY: The next-stage CSRs are staged and this hart's
                // initialized stack is ready for subsequent traps.
                unsafe { enter_stage(hart_id, next_stage.opaque) }
            }
            HartEvent::Park => {
                if let Some(ipi) = ipi {
                    ipi.enable_interrupt(&irq);
                }
                instruction::wait_for_interrupt();
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
///
/// Run in M-mode with interrupts disabled.
/// The next-stage CSRs and Runtime trap stack must already be prepared.
#[unsafe(naked)]
unsafe extern "C" fn enter_stage(hart_id: usize, opaque: usize) -> ! {
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
}
