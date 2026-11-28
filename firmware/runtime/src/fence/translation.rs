//! Local address-translation fence instructions.
//!
//! An omitted operand encodes the register `x0`, selecting all addresses or
//! IDs. A register containing the value zero selects address or ID zero.

use crate::instruction::arch;

/// Synchronizes local address-translation state after a PMP or translation-control update.
///
/// On harts with S, this covers every supervisor address and ASID. A Bare-only
/// hart may trap on `SFENCE.VMA`. On harts with H, the fence also covers all
/// guest physical addresses and VMIDs.
///
/// # Safety
///
/// Run in M-mode with Runtime trap recovery ready, `MPRV` clear, and local
/// interrupts disabled. Preserve any live trap-return context across recovery.
pub unsafe fn synchronize_translations() {
    use csr::{Read, standard::isa::privileged::machine::Misa};

    let isa = Misa::read();
    if isa.has_extension('S') {
        // SAFETY: The caller establishes Runtime recovery under local IRQ exclusion.
        unsafe { try_sfence_vma(None, None) };
    }
    if isa.has_extension('H') {
        hfence_gvma(None, None);
    }
}

/// Returns whether `SFENCE.VMA` completed without an illegal-instruction exception.
///
/// # Safety
///
/// Establish Runtime's M-mode trap recovery ABI, preserve any live trap-return
/// context, and exclude local interrupts.
#[inline]
pub(super) unsafe fn try_sfence_vma(address: Option<usize>, asid: Option<usize>) -> bool {
    macro_rules! attempt {
        ($instruction:literal $(, $name:ident = $operand:expr)*) => {{
            let cause: usize;
            // SAFETY: The M-mode caller establishes Runtime recovery if the instruction traps.
            unsafe {
                core::arch::asm!(
                    "2:", $instruction, "3:", csr::recovery_record!(),
                    causes = const 1 << 2,
                    $($name = in(reg) $operand,)*
                    inout("t2") 0usize => cause,
                    out("t3") _,
                );
            }
            cause == 0
        }};
    }
    match (address, asid) {
        (None, None) => attempt!("sfence.vma x0, x0"),
        (None, Some(asid)) => attempt!("sfence.vma x0, {asid}", asid = asid),
        (Some(address), None) => attempt!("sfence.vma {address}, x0", address = address),
        (Some(address), Some(asid)) => {
            attempt!(
                "sfence.vma {address}, {asid}",
                address = address,
                asid = asid
            )
        }
    }
}

#[inline]
pub(super) fn sfence_vma_page(address: usize, asid: Option<usize>) {
    // SAFETY: The first fence in this range succeeded on the current hart.
    unsafe {
        match asid {
            None => arch::sfence_vma_vaddr(address),
            Some(asid) => arch::sfence_vma(address, asid),
        }
    }
}

#[inline]
pub(super) fn hfence_gvma(address: Option<usize>, vmid: Option<usize>) {
    // HFENCE.GVMA encodes a guest physical address in units of four bytes.
    let address = address.map(|address| address >> 2);
    // SAFETY: The executor checks H support before synchronizing G-stage translations.
    unsafe {
        match (address, vmid) {
            (None, None) => arch::hfence_gvma_all(),
            (None, Some(vmid)) => arch::hfence_gvma_vmid(vmid),
            (Some(address), None) => arch::hfence_gvma_gaddr(address),
            (Some(address), Some(vmid)) => arch::hfence_gvma(address, vmid),
        }
    }
}

#[inline]
pub(super) fn hfence_vvma(address: Option<usize>, asid: Option<usize>) {
    // SAFETY: The executor checks H support and selects the requested guest context.
    unsafe {
        match (address, asid) {
            (None, None) => arch::hfence_vvma_all(),
            (None, Some(asid)) => arch::hfence_vvma_asid(asid),
            (Some(address), None) => arch::hfence_vvma_vaddr(address),
            (Some(address), Some(asid)) => arch::hfence_vvma(address, asid),
        }
    }
}
