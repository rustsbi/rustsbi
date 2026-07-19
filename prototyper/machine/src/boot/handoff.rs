//! Final architectural entry and warm-hart handoff paths.

use super::{NextMode, NextStage};

pub(crate) fn enter(next_stage: NextStage, hart_id: usize, opaque_override: Option<usize>) -> ! {
    const MSTATUS_MPP: usize = 0b11 << 11;
    const MSTATUS_MPIE: usize = 1 << 7;
    const MSTATUS_MPRV: usize = 1 << 17;
    const SSTATUS_SIE: usize = 1 << 1;

    // Soundness invariant: terminal publication forbids any later next-stage
    // entry, even when this hart already completed ordinary preparation.
    if crate::power::is_terminal() {
        crate::power::halt();
    }
    let (entry, opaque, mode) = next_stage.into_parts();
    let opaque = opaque_override.unwrap_or(opaque);
    let mode = match mode {
        NextMode::User => 0,
        NextMode::Supervisor => 1,
        NextMode::Machine => 3,
    };
    let clear = MSTATUS_MPP | MSTATUS_MPIE | MSTATUS_MPRV | SSTATUS_SIE;
    let set = (mode << 11) | MSTATUS_MPIE;
    // SAFETY: terminal preparation validated the mode, entry, DTB ownership,
    // trap state, delegation, and protection policy. No Rust owner is reachable
    // after this non-returning architectural transfer.
    unsafe {
        core::arch::asm!(
            "csrc mstatus, {clear}",
            "csrs mstatus, {set}",
            // HSM start and non-retentive resume enter with address
            // translation disabled. Clearing SIE in the same terminal path
            // makes the supervisor entry contract independent of caller state.
            "csrw satp, zero",
            "sfence.vma",
            "csrw mepc, {entry}",
            "mv a0, {hart_id}",
            "mv a1, {opaque}",
            "mv a2, zero",
            "mret",
            clear = in(reg) clear,
            set = in(reg) set,
            entry = in(reg) entry,
            hart_id = in(reg) hart_id,
            opaque = in(reg) opaque,
            options(noreturn),
        )
    }
}

pub(crate) fn enter_warm_hart(hart_id: usize, index: usize) -> ! {
    // Soundness invariant: a warm hart Acquire-observes terminal state before
    // activating trap policy or considering an HSM start.
    if crate::power::is_terminal() {
        crate::power::halt();
    }
    if crate::trap::entry::activate(index).is_err() {
        crate::trap::abort();
    }
    if crate::trap::entry::prepare_hypervisor_metadata().is_err() {
        crate::trap::abort();
    }
    let Some(runtime) = crate::hart::runtime::runtime() else {
        crate::trap::abort();
    };
    if runtime.prepare_current_hart().is_err() {
        crate::trap::abort();
    }
    #[cfg(feature = "mtest")]
    crate::test_support::mark_warm_parked();
    enable_machine_notification(runtime.notification());

    loop {
        if crate::power::is_terminal() {
            crate::power::halt();
        }
        let Some(mode) = runtime.pending_start_mode(hart_id) else {
            // SAFETY: the dedicated stack and trap vector remain installed;
            // the device's enabled machine notification resumes this wait.
            unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
            continue;
        };
        disable_machine_interrupts();
        let prepared = crate::trap::entry::prepare_timer(index)
            .map_err(|_| crate::HartError::Failed)
            .and_then(|()| {
                crate::pmp::configure_current_hart().map_err(|_| crate::HartError::Failed)
            })
            .and_then(|()| {
                crate::trap::entry::prepare_counters(index, mode)
                    .map_err(|_| crate::HartError::Failed)
            })
            .and_then(|()| crate::interrupt::prepare(mode).map_err(|_| crate::HartError::Failed));
        if runtime.publish_start_result(hart_id, prepared).is_err() {
            crate::trap::abort();
        }
        if prepared.is_err() {
            while runtime.status(hart_id) != Ok(crate::HartStatus::Stopped) {
                core::hint::spin_loop();
            }
            enable_machine_notification(runtime.notification());
            continue;
        }
        let next_stage = match runtime.take_start(hart_id) {
            Ok(next_stage) => next_stage,
            Err(_) => crate::trap::abort(),
        };
        enter(next_stage, hart_id, None)
    }
}

fn enable_machine_notification(notification: crate::hart::Notification) {
    const MIE: usize = 1 << 3;
    let notification = notification.machine_interrupt_bit();
    // SAFETY: the hart trap state, the selected device file, and its matching
    // machine-notification handler are complete before these local enable bits
    // are set. Replacing `mie` also closes every unrelated interrupt source
    // inherited from reset or an earlier supervisor activation.
    unsafe {
        core::arch::asm!(
            "csrw mie, {notification}",
            "csrs mstatus, {mie}",
            notification = in(reg) notification,
            mie = in(reg) MIE,
            options(nostack),
        )
    }
}

fn disable_machine_interrupts() {
    const MIE: usize = 1 << 3;
    // SAFETY: this changes only the calling hart's global machine-interrupt
    // enable during its non-reentrant local preparation transition.
    unsafe { core::arch::asm!("csrc mstatus, {mie}", mie = in(reg) MIE, options(nostack)) };
}
