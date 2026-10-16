//! Local instruction and address-translation fences used by SBI policy.
//!
//! Runtime executes the privileged instructions. SBI policy retains request
//! validation, range selection, remote queues, and PMU event accounting.
//! Hypervisor fences require H; accesses on unsupported hardware trap.

use crate::{Error, Result, hart::Hart, irq};
use csr::{hypervisor::GuestAddressTranslation, machine};

/// Returns the calling hart's virtual machine ID for a remote guest fence.
pub fn current_vmid() -> Result<usize> {
    if !Hart::current()?.has_hypervisor() {
        return Err(Error::NotSupported);
    }
    Ok(GuestAddressTranslation::read().vmid())
}

// Only Runtime's fence instructions run with the temporary guest context.
fn with_vmid(vmid: usize, fence_fn: impl FnOnce()) {
    assert!(crate::boot::traps_ready());
    assert_eq!(usize::from(machine::Scratch::read()), 0);
    assert_eq!(
        machine::Status::read().read::<machine::MPRV>(),
        machine::AccessPrivilege::Current
    );
    irq::with_local_disabled(|_| {
        let previous = GuestAddressTranslation::read();
        // SAFETY: MPRV is clear and IRQs are excluded. Only the VMID changes;
        // no guest execution or memory access occurs before restoration.
        unsafe { GuestAddressTranslation::write(previous.with_vmid(vmid)) };
        fence_fn();
        // SAFETY: Restore the saved context before leaving this IRQ scope.
        unsafe { GuestAddressTranslation::write(previous) };
    });
}

macro_rules! instruction {
    ($instruction:literal $(, in(reg) $operand:expr)*) => {{
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: These fences invalidate cached instructions or translations;
        // they do not modify the live page tables or privilege context.
        unsafe {
            core::arch::asm!(
                ".option push",
                ".option arch, +h",
                $instruction,
                ".option pop"
                $(, in(reg) $operand)*
            );
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            $(let _ = $operand;)*
            unimplemented!("fence instructions require RISC-V");
        }
    }};
}

/// Invalidates the current hart's instruction cache.
pub fn fence_i() {
    riscv::asm::fence_i();
}

/// Invalidates all supervisor TLB entries (`sfence.vma`).
pub fn sfence_vma_all() {
    // SAFETY: Full TLB invalidate; requested by a validated SBI rfence call.
    instruction!("sfence.vma");
}

/// Invalidates supervisor TLB entries for `addr` (`sfence.vma addr`).
pub fn sfence_vma_addr(addr: usize) {
    // SAFETY: Partial TLB invalidate; operands come from a validated
    // SBI rfence call.
    instruction!("sfence.vma {}", in(reg) addr);
}

/// Invalidates all supervisor TLB entries for `asid` (`sfence.vma x0,
/// asid`).
pub fn sfence_vma_asid(asid: usize) {
    // SAFETY: Per-ASID TLB invalidate; requested by a validated SBI rfence call.
    instruction!("sfence.vma x0, {}", in(reg) asid);
}

/// Invalidates supervisor TLB entries for (`addr`, `asid`) (`sfence.vma
/// addr, asid`).
pub fn sfence_vma_addr_asid(addr: usize, asid: usize) {
    // SAFETY: Partial, per-ASID TLB invalidate; operands come from a
    // validated SBI rfence call.
    instruction!("sfence.vma {}, {}", in(reg) addr, in(reg) asid);
}

/// Invalidates all guest TLB entries (`hfence.gvma x0, x0`).
pub fn hfence_gvma_all() {
    // SAFETY: Guest-TLB invalidate; the hypervisor extension probe gates
    // every call site.
    instruction!("hfence.gvma x0, x0");
}

/// Invalidates guest TLB entries for `addr` (`hfence.gvma addr, x0`).
pub fn hfence_gvma_addr(addr: usize) {
    // SAFETY: Single-page guest-physical TLB invalidate; the hypervisor
    // extension probe gates every call site.
    instruction!("hfence.gvma {}, x0", in(reg) addr >> 2);
}

/// Invalidates all guest TLB entries for `vmid` (`hfence.gvma x0, vmid`).
pub fn hfence_gvma_vmid(vmid: usize) {
    // SAFETY: Per-VMID guest-physical TLB invalidate; the hypervisor
    // extension probe gates every call site.
    instruction!("hfence.gvma x0, {}", in(reg) vmid);
}

/// Invalidates guest TLB entries for (`addr`, `vmid`) (`hfence.gvma addr,
/// vmid`).
pub fn hfence_gvma_addr_vmid(addr: usize, vmid: usize) {
    // SAFETY: Partial, per-VMID guest-physical TLB invalidate; the
    // hypervisor extension probe gates every call site.
    instruction!("hfence.gvma {}, {}", in(reg) addr >> 2, in(reg) vmid);
}

/// Invalidates all guest supervisor TLB entries (`hfence.vvma x0, x0`).
pub fn hfence_vvma_all(vmid: usize) {
    // SAFETY: Guest-TLB invalidate; the hypervisor extension probe gates
    // every call site.
    with_vmid(vmid, || instruction!("hfence.vvma x0, x0"));
}

/// Invalidates guest supervisor TLB entries for `addr` (`hfence.vvma addr,
/// x0`).
pub fn hfence_vvma_addr(addr: usize, vmid: usize) {
    // SAFETY: Single-page guest-virtual TLB invalidate; the hypervisor
    // extension probe gates every call site.
    with_vmid(vmid, || instruction!("hfence.vvma {}, x0", in(reg) addr));
}

/// Invalidates all guest supervisor TLB entries for `asid` (`hfence.vvma
/// x0, asid`).
pub fn hfence_vvma_asid(asid: usize, vmid: usize) {
    // SAFETY: Per-ASID guest-virtual TLB invalidate; the hypervisor
    // extension probe gates every call site.
    with_vmid(vmid, || instruction!("hfence.vvma x0, {}", in(reg) asid));
}

/// Invalidates guest supervisor TLB entries for (`addr`, `asid`)
/// (`hfence.vvma addr, asid`).
pub fn hfence_vvma_addr_asid(addr: usize, asid: usize, vmid: usize) {
    // SAFETY: Partial, per-ASID guest-virtual TLB invalidate; the
    // hypervisor extension probe gates every call site.
    with_vmid(
        vmid,
        || instruction!("hfence.vvma {}, {}", in(reg) addr, in(reg) asid),
    );
}
