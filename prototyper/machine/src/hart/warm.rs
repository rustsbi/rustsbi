//! Secondary-hart preparation and wait loop.

use crate::hart::Notification;

/// Activates one published warm hart and waits for HSM work.
pub(crate) fn run(hart_id: usize, index: usize) -> ! {
    if crate::power::is_terminal() {
        crate::power::halt();
    }
    if crate::trap::activate(index).is_err() || crate::trap::prepare_hypervisor_metadata().is_err()
    {
        crate::trap::abort();
    }
    let Some(runtime) = super::protocol::installed() else {
        crate::trap::abort();
    };
    if runtime.prepare_current_hart().is_err() {
        crate::trap::abort();
    }
    enable_notification(runtime.notification());

    loop {
        if crate::power::is_terminal() {
            crate::power::halt();
        }
        let Some(mode) = runtime.pending_start_mode(hart_id) else {
            // SAFETY: this hart retains its dedicated stack and trap vector.
            unsafe { core::arch::asm!("wfi", options(nomem, nostack)) };
            continue;
        };
        mask_notifications();
        let prepared = crate::trap::prepare_timer()
            .map_err(|_| crate::HartError::Failed)
            .and_then(|()| {
                crate::pmp::configure_current_hart().map_err(|_| crate::HartError::Failed)
            })
            .and_then(|()| {
                crate::trap::prepare_counters(mode).map_err(|_| crate::HartError::Failed)
            })
            .and_then(|()| {
                crate::trap::prepare_delegation(mode).map_err(|_| crate::HartError::Failed)
            });
        if runtime.publish_start_result(hart_id, prepared).is_err() {
            crate::trap::abort();
        }
        if prepared.is_err() {
            while runtime.status(hart_id) != Ok(crate::HartStatus::Stopped) {
                core::hint::spin_loop();
            }
            enable_notification(runtime.notification());
            continue;
        }
        let next_stage = match runtime.take_start(hart_id) {
            Ok(next_stage) => next_stage,
            Err(_) => crate::trap::abort(),
        };
        next_stage.transfer(hart_id, None)
    }
}

fn enable_notification(notification: Notification) {
    const MIE: usize = 1 << 3;
    let bit = notification.machine_interrupt_bit();
    // SAFETY: this hart's matching notification device and trap policy are
    // complete before the local machine interrupt source is enabled.
    unsafe {
        core::arch::asm!(
            "csrw mie, {bit}",
            "csrs mstatus, {mie}",
            bit = in(reg) bit,
            mie = in(reg) MIE,
            options(nostack),
        )
    }
}

fn mask_notifications() {
    const MIE: usize = 1 << 3;
    // SAFETY: local preparation cannot be interrupted by its notification.
    unsafe { core::arch::asm!("csrc mstatus, {mie}", mie = in(reg) MIE, options(nostack)) };
}
