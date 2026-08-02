//! RISC-V operations used after hart protocol state has committed.

use super::fence::RemoteFenceRequest;
use super::ipi::Notification;

pub(in crate::hart) fn protocol_fence() {
    // SAFETY: the full device fence carries no memory operand. Default asm
    // options retain compiler memory effects around normal-memory state.
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) }
}

pub(in crate::hart) fn manifest_supervisor_ipi() {
    // SAFETY: setting the supervisor software-pending bit is the architectural
    // manifestation of a claimed ordinary IPI for the current supervisor.
    unsafe { core::arch::asm!("csrsi mip, 2", options(nostack)) }
}

pub(in crate::hart) fn clear_supervisor_ipi() {
    // SAFETY: stop has relinquished the old supervisor context. Clearing its
    // software-pending manifestation prevents leakage into a later start.
    unsafe { core::arch::asm!("csrci mip, 2", options(nostack)) }
}

pub(in crate::hart) fn wait_for_wake_event(notification: Notification) {
    let wake = notification.machine_interrupt_bit();
    let _previous: usize;
    // SAFETY: the added bit matches the initialized notification device,
    // MSTATUS.MIE remains clear, and WFI may return spuriously.
    unsafe {
        core::arch::asm!(
            "csrr {previous}, mie",
            "or {wake}, {previous}, {wake}",
            "csrw mie, {wake}",
            "wfi",
            "csrw mie, {previous}",
            previous = out(reg) _previous,
            wake = inout(reg) wake => _,
            options(nomem, nostack),
        )
    }
}

pub(in crate::hart) fn execute(request: RemoteFenceRequest) {
    match request {
        RemoteFenceRequest::FenceI => {
            // SAFETY: executes the closed remote instruction-fence request.
            unsafe { core::arch::asm!("fence.i", options(nostack)) }
        }
        RemoteFenceRequest::SfenceVma { .. } => {
            // SAFETY: a global fence is stronger than the requested range.
            unsafe { core::arch::asm!("sfence.vma", options(nostack)) }
        }
        RemoteFenceRequest::SfenceVmaAsid { asid, .. } => {
            // SAFETY: x0 selects a stronger full-address flush for this ASID.
            unsafe {
                core::arch::asm!("sfence.vma zero, {asid}", asid = in(reg) asid, options(nostack))
            }
        }
        #[cfg(feature = "hypervisor")]
        RemoteFenceRequest::HfenceGvma { .. } => {
            // SAFETY: target admission proved that this hart implements H.
            unsafe {
                core::arch::asm!(
                    ".option push",
                    ".option arch,+h",
                    "hfence.gvma",
                    ".option pop",
                    options(nostack),
                )
            }
        }
        #[cfg(feature = "hypervisor")]
        RemoteFenceRequest::HfenceGvmaVmid { vmid, .. } => {
            // SAFETY: target admission proved H; x0 selects a full-VM flush.
            unsafe {
                core::arch::asm!(
                    ".option push",
                    ".option arch,+h",
                    "hfence.gvma zero, {vmid}",
                    ".option pop",
                    vmid = in(reg) vmid,
                    options(nostack),
                )
            }
        }
        #[cfg(feature = "hypervisor")]
        RemoteFenceRequest::HfenceVvma { .. } => {
            // SAFETY: target admission proved that this hart implements H.
            unsafe {
                core::arch::asm!(
                    ".option push",
                    ".option arch,+h",
                    "hfence.vvma",
                    ".option pop",
                    options(nostack),
                )
            }
        }
        #[cfg(feature = "hypervisor")]
        RemoteFenceRequest::HfenceVvmaAsid { asid, .. } => {
            // SAFETY: target admission proved H; x0 selects a full-ASID flush.
            unsafe {
                core::arch::asm!(
                    ".option push",
                    ".option arch,+h",
                    "hfence.vvma zero, {asid}",
                    ".option pop",
                    asid = in(reg) asid,
                    options(nostack),
                )
            }
        }
    }
}

pub(in crate::hart) fn current_hart_id() -> usize {
    let value;
    // SAFETY: `mhartid` is a mandatory read-only machine CSR.
    unsafe {
        core::arch::asm!("csrr {value}, mhartid", value = out(reg) value, options(nomem, nostack))
    };
    value
}

pub(in crate::hart) fn mask_protocol_interrupts() -> usize {
    const PROTOCOL_INTERRUPTS: usize = (1 << 3) | (1 << 11);
    let previous: usize;
    // SAFETY: masks only the two protocol notification sources locally.
    unsafe {
        core::arch::asm!(
            "csrrc {previous}, mie, {mask}",
            previous = out(reg) previous,
            mask = in(reg) PROTOCOL_INTERRUPTS,
            options(nostack),
        )
    };
    previous & PROTOCOL_INTERRUPTS
}

pub(in crate::hart) fn restore_protocol_interrupts(previous: usize) {
    // SAFETY: restores only bits captured from this hart after lock release.
    unsafe { core::arch::asm!("csrs mie, {mask}", mask = in(reg) previous, options(nostack)) }
}

pub(in crate::hart) fn mask_all_interrupts() -> usize {
    let previous: usize;
    // SAFETY: captures and clears only the calling hart's `mie` CSR.
    unsafe {
        core::arch::asm!(
            "csrrw {previous}, mie, zero",
            previous = out(reg) previous,
            options(nostack),
        )
    };
    previous
}

pub(in crate::hart) fn restore_all_interrupts(previous: usize) {
    // SAFETY: restores the exact local value captured by `mask_all_interrupts`.
    unsafe {
        core::arch::asm!("csrw mie, {previous}", previous = in(reg) previous, options(nostack))
    }
}
