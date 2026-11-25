//! Instruction and address-translation fences on the current hart.

use crate::{Error, Result};
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use crate::{hart, instruction, irq};
use csr::values::xlen::Xlen;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use csr::{
    hypervisor::{GuestAddressSpace, GuestAddressTranslation},
    supervisor::AddressSpace,
    virtual_supervisor::AddressTranslation as VirtualAddressTranslation,
};

#[cfg(target_pointer_width = "32")]
const NATIVE_XLEN: Xlen = Xlen::X32;
#[cfg(target_pointer_width = "64")]
const NATIVE_XLEN: Xlen = Xlen::X64;

const fn fits_bits(value: usize, bits: u32) -> bool {
    value < (1usize << bits)
}

/// Returns whether `asid` fits within the RISC-V privileged architecture's maximum ASID width.
pub const fn is_valid_asid(asid: usize) -> bool {
    is_valid_asid_for(asid, NATIVE_XLEN)
}

/// Returns whether `asid` fits within the maximum ASID width for `xlen`.
pub const fn is_valid_asid_for(asid: usize, xlen: Xlen) -> bool {
    match xlen {
        Xlen::X32 => fits_bits(asid, 9),
        Xlen::X64 => fits_bits(asid, 16),
        Xlen::X128 => false,
    }
}

/// Returns whether `vmid` fits within the RISC-V privileged architecture's maximum VMID width.
pub const fn is_valid_vmid(vmid: usize) -> bool {
    is_valid_vmid_for(vmid, NATIVE_XLEN)
}

/// Returns whether `vmid` fits within the maximum VMID width for `xlen`.
pub const fn is_valid_vmid_for(vmid: usize, xlen: Xlen) -> bool {
    match xlen {
        Xlen::X32 => fits_bits(vmid, 7),
        Xlen::X64 => fits_bits(vmid, 14),
        Xlen::X128 => false,
    }
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
macro_rules! hfence {
    ($mnemonic:literal, $address:expr, $id:expr) => {
        match ($address, $id) {
            (None, None) => hfence!(@emit $mnemonic, "x0, x0"),
            (None, Some(id)) => hfence!(@emit $mnemonic, "x0, {}", id),
            (Some(address), None) => hfence!(@emit $mnemonic, "{}, x0", address),
            (Some(address), Some(id)) => hfence!(@emit $mnemonic, "{}, {}", address, id),
        }
    };
    (@emit $mnemonic:literal, $operands:literal $(, $value:expr)*) => {
        core::arch::asm!(
            ".option push",
            ".option arch, +h",
            concat!($mnemonic, " ", $operands),
            ".option pop",
            $(in(reg) $value,)*
            options(nostack),
        )
    };
}

/// Addresses covered by an address-translation fence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlushRange {
    /// All addresses in the specified translation context.
    All,
    /// Page addresses at offsets smaller than `size`, with wrapping addition.
    Pages {
        /// The first page address.
        start: usize,
        /// The extent in bytes. Zero selects no pages.
        size: usize,
        /// The distance between page addresses in bytes. Nonempty ranges require a nonzero stride.
        stride: usize,
    },
}

/// A fence to execute on the current hart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalFence {
    /// Synchronizes instruction fetches with prior stores.
    Instruction,
    /// Synchronizes supervisor virtual address translations.
    Supervisor {
        /// The supervisor virtual addresses to synchronize.
        range: FlushRange,
        /// The ASID to synchronize, or `None` to include all ASIDs.
        asid: Option<usize>,
    },
    /// Synchronizes guest physical address translations.
    GuestPhysical {
        /// The guest physical byte addresses to synchronize.
        range: FlushRange,
        /// The VMID to synchronize, or `None` to include all VMIDs.
        vmid: Option<usize>,
    },
    /// Synchronizes guest virtual address translations for one guest.
    GuestVirtual {
        /// The guest virtual addresses to synchronize.
        range: FlushRange,
        /// The ASID to synchronize, or `None` to include all ASIDs.
        asid: Option<usize>,
        /// The VMID identifying the guest address space.
        vmid: usize,
    },
}

fn page_addresses(start: usize, size: usize, stride: usize) -> Result<impl Iterator<Item = usize>> {
    if size > 0 && stride == 0 {
        return Err(Error::InvalidArgs);
    }
    Ok((0..size)
        .step_by(stride.max(1))
        .map(move |offset| start.wrapping_add(offset)))
}

/// Reads the current hart's active guest VMID from `hgatp` using its `HSXLEN`.
///
/// Returns [`Error::NotSupported`] if the hart lacks the H extension,
/// `HSXLEN` is unavailable, or the build target is outside RISC-V.
/// Call from M-mode.
pub fn current_guest_vmid() -> Result<usize> {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        require_hypervisor()?;
        let hsxlen = hart::supervisor_xlen().ok_or(Error::NotSupported)?;
        GuestAddressTranslation::read()
            .vmid_for(hsxlen)
            .ok_or(Error::NotSupported)
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    Err(Error::NotSupported)
}

/// Executes a fence on the current hart.
///
/// Call from M-mode with `MPRV` clear.
/// Guest virtual fences exclude local interrupts while the requested guest VMID is active.
///
/// Returns [`Error::InvalidArgs`] for a nonempty page range with zero stride,
/// or [`Error::NotSupported`] on a non-RISC-V target, when a guest fence is
/// requested on a hart without the H extension, or when `vmid` cannot be encoded
/// for the target hart's `HSXLEN`.
pub fn execute_local(fence: LocalFence) -> Result<()> {
    let range = match fence {
        LocalFence::Instruction => FlushRange::All,
        LocalFence::Supervisor { range, .. }
        | LocalFence::GuestPhysical { range, .. }
        | LocalFence::GuestVirtual { range, .. } => range,
    };
    let addresses = match range {
        FlushRange::All => None,
        FlushRange::Pages {
            start,
            size,
            stride,
        } => Some(page_addresses(start, size, stride)?),
    };

    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        match fence {
            LocalFence::Instruction => instruction::fence_i(),
            LocalFence::Supervisor { asid, .. } => {
                for_each_address(addresses, |address| match (address, asid) {
                    (None, None) => instruction::sfence_vma_all(),
                    (None, Some(asid)) => instruction::sfence_vma_asid(asid),
                    (Some(address), None) => instruction::sfence_vma_addr(address),
                    (Some(address), Some(asid)) => instruction::sfence_vma(asid, address),
                });
            }
            LocalFence::GuestPhysical { vmid, .. } => {
                require_hypervisor()?;
                for_each_address(addresses, |address| {
                    // SAFETY: The current hart supports H.
                    unsafe { hfence!("hfence.gvma", address.map(hfence_gvma_operand), vmid) };
                });
            }
            LocalFence::GuestVirtual { asid, vmid, .. } => {
                require_hypervisor()?;
                let hsxlen = hart::supervisor_xlen().ok_or(Error::NotSupported)?;
                let target_hgatp =
                    GuestAddressSpace::for_vmid(vmid, hsxlen).ok_or(Error::NotSupported)?;
                irq::with_local_disabled(|irq| {
                    // SAFETY: H is available, local interrupts are disabled,
                    // `target_hgatp` encodes `vmid` for `hsxlen` with Bare mode,
                    // and Runtime executes with MPRV clear.
                    unsafe {
                        with_guest_vmid(irq, vmid, hsxlen, target_hgatp, || {
                            for_each_address(addresses, |address| {
                                hfence!("hfence.vvma", address, asid);
                            });
                        });
                    }
                });
            }
        }
        Ok(())
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = addresses;
        Err(Error::NotSupported)
    }
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
fn for_each_address(
    addresses: Option<impl Iterator<Item = usize>>,
    mut for_each_fn: impl FnMut(Option<usize>),
) {
    match addresses {
        Some(addresses) => addresses.map(Some).for_each(for_each_fn),
        None => for_each_fn(None),
    }
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
fn require_hypervisor() -> Result<()> {
    if hart::has_hypervisor() {
        Ok(())
    } else {
        Err(Error::NotSupported)
    }
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
fn hfence_gvma_operand(address: usize) -> usize {
    // HFENCE.GVMA encodes the guest physical address in four-byte units.
    address >> 2
}

/// Executes `fence_fn` with `vmid` selected in `hgatp` under `hsxlen`.
///
/// # Safety
///
/// Call in M-mode with `MPRV` clear on a hart with the H extension while local
/// interrupts are disabled and `target_hgatp` encodes `vmid` for `hsxlen` in
/// Bare translation mode.
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
unsafe fn with_guest_vmid(
    _irq: &irq::LocalIrqDisabled,
    vmid: usize,
    hsxlen: Xlen,
    target_hgatp: GuestAddressSpace,
    fence_fn: impl FnOnce(),
) {
    let saved_hgatp = GuestAddressTranslation::read();
    if saved_hgatp.vmid_for(hsxlen) == Some(vmid) {
        fence_fn();
        return;
    }

    let saved_vsatp = VirtualAddressTranslation::read();
    // SAFETY: Local interrupts are excluded and MPRV is clear. Setting vsatp
    // to Bare prevents speculative G-stage walks from using the old guest's table,
    // and switching `hgatp` with Bare mode updates only VMID for HFENCE.VVMA.
    unsafe {
        VirtualAddressTranslation::write(AddressSpace::bare());
        GuestAddressTranslation::write(target_hgatp);
    }
    fence_fn();
    // SAFETY: Restore the original `hgatp` before reactivating `vsatp`.
    unsafe {
        GuestAddressTranslation::write(saved_hgatp);
        VirtualAddressTranslation::write(saved_vsatp);
    }
}

#[cfg(test)]
mod tests {
    use super::page_addresses;

    #[test]
    fn page_addresses_cover_partial_pages_and_wrap() {
        let mut pages = page_addresses(usize::MAX - 0xfff, 0x2001, 0x1000).unwrap();
        assert_eq!(pages.next(), Some(usize::MAX - 0xfff));
        assert_eq!(pages.next(), Some(0));
        assert_eq!(pages.next(), Some(0x1000));
        assert_eq!(pages.next(), None);
    }
}
