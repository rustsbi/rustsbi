//! Private, recoverable fence instructions. Call only under Runtime IRQ exclusion.

use crate::{Error, Result, irq::LocalIrqDisabled};

fn architectural_asid(asid: usize) -> usize {
    // LocalFence checks SXLEN == MXLEN, and also VSXLEN == MXLEN for VVMA,
    // before any nonempty request reaches these native-width encodings.
    let asid_bits = if usize::BITS == 32 { 9 } else { 16 };
    asid & ((1 << asid_bits) - 1)
}

#[cfg(feature = "hypervisor")]
fn architectural_vmid(vmid: usize) -> usize {
    // Nonempty GVMA and borrowed-guest operations require HSXLEN == MXLEN.
    let vmid_bits = if usize::BITS == 32 { 7 } else { 14 };
    vmid & ((1 << vmid_bits) - 1)
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
macro_rules! fence {
    ($instruction:literal $(, $name:ident = $value:expr)*) => {{
        let cause: usize;
        // SAFETY: LocalFence checked Runtime entry and the current hart, and
        // excludes interrupts for the complete operation. M-mode Rust runs
        // with mscratch=0 and MPRV=0. Only this instruction's illegal-instruction
        // fault is recoverable; all trap metadata and declared clobbers survive.
        // Do not use nomem/readonly: these instructions order memory accesses.
        unsafe {
            core::arch::asm!(
                "li t2, 0",
                ".option push",
                ".option norvc",
                "2:",
                $instruction,
                "3:",
                ".option pop",
                crate::recovery::fault_site!(),
                causes = const 1 << 2,
                $($name = in(reg) $value,)*
                out("t2") cause,
                out("t3") _,
            );
        }
        if cause == 0 { Ok(()) } else { Err(Error::NotSupported) }
    }};
}

#[cfg(all(
    feature = "hypervisor",
    any(target_arch = "riscv32", target_arch = "riscv64")
))]
macro_rules! hfence {
    ($instruction:literal $(, $name:ident = $value:expr)*) => {{
        let cause: usize;
        // SAFETY: LocalFence checked Runtime entry and the current hart, and
        // excludes interrupts for the complete operation. The local assembler
        // option enables encoding H instructions without requiring H globally.
        unsafe {
            core::arch::asm!(
                "li t2, 0",
                ".option push",
                ".option arch, +h",
                ".option norvc",
                "2:",
                $instruction,
                "3:",
                ".option pop",
                crate::recovery::fault_site!(),
                causes = const 1 << 2,
                $($name = in(reg) $value,)*
                out("t2") cause,
                out("t3") _,
            );
        }
        if cause == 0 { Ok(()) } else { Err(Error::NotSupported) }
    }};
}

pub(super) fn fence_i(_irq: &LocalIrqDisabled) -> Result<()> {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        fence!("fence.i")
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        Err(Error::NotSupported)
    }
}

pub(super) fn sfence_vma(
    _irq: &LocalIrqDisabled,
    address: Option<usize>,
    asid: Option<usize>,
) -> Result<()> {
    let asid = asid.map(architectural_asid);
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        match (address, asid) {
            (None, None) => fence!("sfence.vma x0, x0"),
            (Some(address), None) => fence!("sfence.vma {address}, x0", address = address),
            (None, Some(asid)) => fence!("sfence.vma x0, {asid}", asid = asid),
            (Some(address), Some(asid)) => {
                fence!(
                    "sfence.vma {address}, {asid}",
                    address = address,
                    asid = asid
                )
            }
        }
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = (address, asid);
        Err(Error::NotSupported)
    }
}

#[cfg(feature = "hypervisor")]
pub(super) fn hfence_gvma(
    _irq: &LocalIrqDisabled,
    address: Option<usize>,
    vmid: Option<usize>,
) -> Result<()> {
    let vmid = vmid.map(architectural_vmid);
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        let address = address.map(|address| address >> 2);
        match (address, vmid) {
            (None, None) => hfence!("hfence.gvma x0, x0"),
            (Some(address), None) => hfence!("hfence.gvma {address}, x0", address = address),
            (None, Some(vmid)) => hfence!("hfence.gvma x0, {vmid}", vmid = vmid),
            (Some(address), Some(vmid)) => hfence!(
                "hfence.gvma {address}, {vmid}",
                address = address,
                vmid = vmid
            ),
        }
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = (address, vmid);
        Err(Error::NotSupported)
    }
}

#[cfg(feature = "hypervisor")]
/// # Safety-usable invariant
///
/// In this module's Runtime execution/IRQ scope, performs only the requested
/// local VVMA or reports an illegal instruction. It does not access guest
/// memory, change CSRs, enable interrupts, invoke callbacks, reenter, or unwind.
pub(super) fn hfence_vvma(
    _irq: &LocalIrqDisabled,
    address: Option<usize>,
    asid: Option<usize>,
) -> Result<()> {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        let asid = asid.map(architectural_asid);
        match (address, asid) {
            (None, None) => hfence!("hfence.vvma x0, x0"),
            (Some(address), None) => hfence!("hfence.vvma {address}, x0", address = address),
            (None, Some(asid)) => hfence!("hfence.vvma x0, {asid}", asid = asid),
            (Some(address), Some(asid)) => hfence!(
                "hfence.vvma {address}, {asid}",
                address = address,
                asid = asid
            ),
        }
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = (address, asid);
        Err(Error::NotSupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reserved_asid_bits_are_cleared_without_changing_valid_bits() {
        let mask = if usize::BITS == 32 { 0x1ff } else { 0xffff };
        for asid in [0, 1, mask] {
            assert_eq!(architectural_asid(asid), asid);
            assert_eq!(architectural_asid(asid | !mask), asid);
        }
    }

    #[cfg(feature = "hypervisor")]
    #[test]
    fn reserved_vmid_bits_are_cleared_without_changing_valid_bits() {
        let mask = if usize::BITS == 32 { 0x7f } else { 0x3fff };
        for vmid in [0, 1, mask] {
            assert_eq!(architectural_vmid(vmid), vmid);
            assert_eq!(architectural_vmid(vmid | !mask), vmid);
        }
    }
}
