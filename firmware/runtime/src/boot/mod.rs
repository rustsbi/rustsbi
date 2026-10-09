//! Firmware entry, stack setup, and next-stage handoff.
//!
//! Firmware connects its boot policy through [`FirmwareEntry`]. Runtime sets up
//! disjoint hart stacks, then [`finish_boot`] discards the boot call chain and
//! reuses each stack for traps. [`fail_stop`] also works before stacks exist.

use crate::csr::Mie;
mod cold;
mod handoff;
mod images;
pub(crate) mod reset;
mod stack;

pub use cold::{BootInput, BootPolicy, BootStorage, FirmwareEntry, PreparedBoot};
pub use handoff::{DynamicInfo, DynamicReadError};
pub use images::{embedded_fdt, embedded_payload};
pub use reset::{ResetEntry, ResetEntryAlreadyRegistered};
pub(crate) use stack::firmware_end;
pub(crate) use stack::locate_stack;

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

/// Stops this hart without using its stack.
///
/// This is also the early `mtvec` target, before Runtime state exists.
///
/// # Safety
///
/// The caller must run in M-mode. This function never returns or unwinds.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
#[unsafe(export_name = "runtime_fail_stop")]
pub unsafe extern "C" fn fail_stop() -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(".balign 4", "csrw mie, zero", "1: wfi", "   j 1b",);
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("The fail-stop vector requires a RISC-V target");
}

/// Enters the next stage or parks after boot or a hart stop. Never returns.
///
/// Each hart reuses one stack for boot and traps. This operation discards
/// the old call chain, arms the clean stack for traps, and waits for a staged
/// next stage if necessary. Initial boot transitions from Ready to Armed here;
/// hart stop uses the same path to retire the previous supervisor context.
///
/// # Safety
///
/// 1. The caller runs in M-mode with machine interrupts disabled, on a hart
///    with a published Runtime stack and initialized traps.
/// 2. The boot or stopped trap call chain can be discarded without unwinding.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
#[unsafe(export_name = "runtime_finish_boot")]
pub unsafe extern "C" fn finish_boot() -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(
        ".balign 4",
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
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = finish_boot_rust;
        unimplemented!("Boot completion requires a RISC-V target");
    }
}

/// Enters the staged next stage, or parks until one arrives.
fn finish_boot_rust() -> ! {
    let hart = hart::current_hart();
    mark_armed(hart);
    let ipi = match crate::ipi::Ipi::current() {
        Ok(ipi) => Some(ipi),
        Err(crate::ipi::Error::Unavailable) => None,
        Err(error) => panic!("BUG: invalid boot IPI capability: {error}"),
    };
    loop {
        // Acknowledge before observing the state. Clearing after observing
        // Stopped could erase a concurrent start's wakeup just before WFI.
        let event = match ipi.as_ref() {
            Some(ipi) => ipi
                .poll()
                .expect("BUG: could not receive the hart wake interrupt"),
            None => hart::take_local_event(),
        };
        match event {
            HartEvent::Start(next_stage) => {
                // SAFETY:
                // 1. The entry/stop path retains M-mode with MIE clear.
                // 2. This hart's traps and timer are initialized; its cell owns the entry.
                unsafe {
                    crate::trap::dispatch::stage_next_mode(
                        next_stage.start_addr,
                        next_stage.next_mode,
                    )
                };
                let hart_id = hart.as_usize();
                // SAFETY:
                // 1. stage_next_mode prepared the return mode and PC with MIE clear.
                // 2. finish_boot armed this hart's trap stack; this call chain is retired.
                unsafe { enter_stage(hart_id, next_stage.opaque) }
            }
            HartEvent::Park => {
                // Arm the selected IPI transport before waiting for a staged start.
                match ipi.as_ref() {
                    Some(ipi) => ipi
                        .prepare_wait()
                        .expect("BUG: IPI capability used on another hart"),
                    None => Mie::set_bits(Mie::MACHINE_SOFTWARE),
                }
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
///
/// The caller runs in M-mode with MIE clear, with the next-stage CSRs and
/// Runtime trap stack prepared. The current call chain can be discarded.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
unsafe extern "C" fn enter_stage(hart_id: usize, opaque: usize) -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
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
    );
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = (hart_id, opaque);
        unimplemented!("Next-stage entry requires a RISC-V target");
    }
}
