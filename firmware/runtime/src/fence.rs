//! Instruction and address-translation fences on the current hart.
//!
//! [Supervisor translation fences](https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#norm:sfence-vma_op)
//! and [hypervisor translation fences](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#hfence.vma).

use crate::{Error, Result};
use riscv_csr_spec::standard::isa::{
    privileged::hypervisor::GuestAddressSpace, values::xlen::Xlen,
};

cfg_select! {
    any(target_arch = "riscv32", target_arch = "riscv64") => {
        mod firmware;
        mod translation;
        pub use firmware::{current_guest_vmid, execute_local};
        pub use translation::synchronize_translations;
    }
    _ => {}
}

const fn fits_bits(value: usize, bits: u32) -> bool {
    value < (1usize << bits)
}

/// Returns whether `asid` fits within the RISC-V privileged architecture's maximum ASID width.
pub const fn is_valid_asid(asid: usize) -> bool {
    is_valid_asid_for(asid, Xlen::NATIVE)
}

/// Returns whether `asid` fits within the maximum ASID width for `xlen`.
pub const fn is_valid_asid_for(asid: usize, xlen: Xlen) -> bool {
    match xlen {
        Xlen::X32 => fits_bits(asid, 9),
        Xlen::X64 => fits_bits(asid, 16),
    }
}

/// Returns whether `vmid` fits within the RISC-V privileged architecture's maximum VMID width.
pub const fn is_valid_vmid(vmid: usize) -> bool {
    is_valid_vmid_for(vmid, Xlen::NATIVE)
}

/// Returns whether `vmid` fits within the maximum VMID width for `xlen`.
pub const fn is_valid_vmid_for(vmid: usize, xlen: Xlen) -> bool {
    vmid <= GuestAddressSpace::max_vmid_for(xlen)
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

impl FlushRange {
    fn pages(self) -> Result<Option<impl Iterator<Item = usize>>> {
        let Self::Pages {
            start,
            size,
            stride,
        } = self
        else {
            return Ok(None);
        };
        if size > 0 && stride == 0 {
            return Err(Error::InvalidArgs);
        }
        Ok(Some(
            (0..size)
                .step_by(stride.max(1))
                .map(move |offset| start.wrapping_add(offset)),
        ))
    }
}

/// A fence to execute on the current hart.
///
/// Selecting a virtual address covers leaf translations. An ASID-specific
/// fence can leave global mappings cached. For ASIDs and VMIDs, `None`
/// includes every ID and `Some(0)` selects ID zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalFence {
    /// Synchronizes subsequent instruction fetches with stores already visible to this hart.
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

#[cfg(test)]
mod tests {
    use super::FlushRange;

    #[test]
    fn page_addresses_cover_partial_pages_and_wrap() {
        let range = FlushRange::Pages {
            start: usize::MAX - 0xfff,
            size: 0x2001,
            stride: 0x1000,
        };
        let mut pages = range.pages().unwrap().unwrap();
        assert_eq!(pages.next(), Some(usize::MAX - 0xfff));
        assert_eq!(pages.next(), Some(0));
        assert_eq!(pages.next(), Some(0x1000));
        assert_eq!(pages.next(), None);
    }
}
