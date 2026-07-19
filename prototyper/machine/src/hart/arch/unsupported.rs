//! Fail-stop backend for accidental non-RISC-V use outside unit tests.

use super::super::fence::RemoteFenceRequest;
use super::super::ipi::Notification;

fn unavailable() -> ! {
    panic!("hart architecture operations require a RISC-V target")
}

pub(in crate::hart) fn protocol_fence() {
    unavailable()
}

pub(in crate::hart) fn manifest_supervisor_ipi() {
    unavailable()
}

pub(in crate::hart) fn clear_supervisor_ipi() {
    unavailable()
}

pub(in crate::hart) fn wait_for_wake_event(_: Notification) {
    unavailable()
}

pub(in crate::hart) fn execute(_: RemoteFenceRequest) {
    unavailable()
}

pub(in crate::hart) fn current_hart_id() -> usize {
    unavailable()
}

pub(in crate::hart) fn mask_protocol_interrupts() -> usize {
    unavailable()
}

pub(in crate::hart) fn restore_protocol_interrupts(_: usize) {
    unavailable()
}

pub(in crate::hart) fn mask_all_interrupts() -> usize {
    unavailable()
}

pub(in crate::hart) fn restore_all_interrupts(_: usize) {
    unavailable()
}
