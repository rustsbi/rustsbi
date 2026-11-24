//! Instruction and address-translation fences on the current hart.

use crate::{Error, Result};
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use crate::{hart, instruction, irq};
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
use csr::{
    hypervisor::GuestAddressTranslation, supervisor::AddressSpace,
    virtual_supervisor::AddressTranslation as VirtualAddressTranslation,
};

/// A guest address-space snapshot containing the active VMID.
pub use csr::hypervisor::GuestAddressSpace;

#[cfg(target_pointer_width = "32")]
const ASID_MAX_BITS: u32 = 9;
#[cfg(target_pointer_width = "64")]
const ASID_MAX_BITS: u32 = 16;

#[cfg(target_pointer_width = "32")]
const VMID_MAX_BITS: u32 = 7;
#[cfg(target_pointer_width = "64")]
const VMID_MAX_BITS: u32 = 14;

const fn fits_bits(value: usize, bits: u32) -> bool {
    value < (1usize << bits)
}

/// Returns whether `asid` fits within the RISC-V privileged architecture's maximum ASID width.
pub const fn is_valid_asid(asid: usize) -> bool {
    fits_bits(asid, ASID_MAX_BITS)
}

/// Returns whether `vmid` fits within the RISC-V privileged architecture's maximum VMID width.
pub const fn is_valid_vmid(vmid: usize) -> bool {
    fits_bits(vmid, VMID_MAX_BITS)
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
        /// The guest address-space snapshot that selects the VMID.
        guest: GuestAddressSpace,
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

/// Reads the current hart's guest address-space configuration from `hgatp`.
///
/// Returns [`Error::NotSupported`] if the hart lacks the H extension or the
/// build target is outside RISC-V. Call from M-mode.
pub fn guest_address_space() -> Result<GuestAddressSpace> {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        require_hypervisor()?;
        Ok(GuestAddressTranslation::read())
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    Err(Error::NotSupported)
}

/// Executes a fence on the current hart.
///
/// Call from M-mode with `MPRV` clear. Guest virtual fences exclude local
/// interrupts while the requested guest address space is active.
///
/// Returns [`Error::InvalidArgs`] for a nonempty page range with zero stride,
/// or [`Error::NotSupported`] on a non-RISC-V target or when a guest fence
/// is requested on a hart without the H extension.
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
            LocalFence::GuestVirtual { asid, guest, .. } => {
                require_hypervisor()?;
                irq::with_local_disabled(|irq| {
                    // SAFETY: H is available, local interrupts are disabled,
                    // and Runtime executes with MPRV clear.
                    unsafe {
                        with_guest_address_space(irq, guest, || {
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
    mut f: impl FnMut(Option<usize>),
) {
    match addresses {
        Some(addresses) => addresses.map(Some).for_each(f),
        None => f(None),
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

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
#[inline]
unsafe fn hfence_gvma_all() {
    // SAFETY: The caller verified H-extension support.
    unsafe { hfence!(@emit "hfence.gvma", "x0, x0") };
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
unsafe fn with_guest_address_space(
    _irq: &irq::LocalIrqDisabled,
    guest: GuestAddressSpace,
    fence: impl FnOnce(),
) {
    let saved_hgatp = GuestAddressTranslation::read();
    if saved_hgatp == guest {
        fence();
        return;
    }

    let saved_vsatp = VirtualAddressTranslation::read();
    // SAFETY: Local interrupts are excluded and MPRV is clear. Setting vsatp
    // to Bare prevents speculative walks from using the old guest's table.
    unsafe {
        VirtualAddressTranslation::write(AddressSpace::bare());
        GuestAddressTranslation::write(guest);
        hfence_gvma_all();
    }
    fence();
    // SAFETY: Restore the original hgatp and order its mode change before
    // reactivating the original guest page table.
    unsafe {
        GuestAddressTranslation::write(saved_hgatp);
        hfence_gvma_all();
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
