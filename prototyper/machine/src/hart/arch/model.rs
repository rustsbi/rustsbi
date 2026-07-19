//! Deterministic host model for hart protocol unit tests.

use super::super::fence::RemoteFenceRequest;
use super::super::ipi::Notification;

pub(in crate::hart) fn protocol_fence() {}

pub(in crate::hart) fn manifest_supervisor_ipi() {}

pub(in crate::hart) fn clear_supervisor_ipi() {}

pub(in crate::hart) fn wait_for_wake_event(notification: Notification) {
    let _ = notification.machine_interrupt_bit();
    core::hint::spin_loop();
}

pub(in crate::hart) fn execute(_: RemoteFenceRequest) {}

pub(in crate::hart) fn current_hart_id() -> usize {
    0
}

pub(in crate::hart) fn mask_protocol_interrupts() -> usize {
    0
}

pub(in crate::hart) fn restore_protocol_interrupts(_: usize) {}

pub(in crate::hart) fn mask_all_interrupts() -> usize {
    0
}

pub(in crate::hart) fn restore_all_interrupts(_: usize) {}
