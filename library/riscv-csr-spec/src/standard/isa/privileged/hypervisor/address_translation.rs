//! Guest physical address translation and protection.

use crate::standard::isa::values::xlen::Xlen;

/// Defined paged G-stage address-translation modes in `hgatp.MODE`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GuestPagingMode {
    /// RV32 page-based translation of 34-bit guest physical addresses.
    Sv32x4 = 1,
    /// RV64 page-based translation of 41-bit guest physical addresses.
    Sv39x4 = 8,
    /// RV64 page-based translation of 50-bit guest physical addresses.
    Sv48x4 = 9,
    /// RV64 page-based translation of 59-bit guest physical addresses.
    Sv57x4 = 10,
}

csr_value! {
    /// Hypervisor guest address translation and protection register value (`hgatp`).
    pub struct GuestAddressSpace;
}

impl GuestAddressSpace {
    /// Returns the maximum architectural VMID for `HSXLEN`, before WARL narrowing.
    #[inline]
    pub const fn max_vmid_for(xlen: Xlen) -> usize {
        match xlen {
            Xlen::X32 => 0x7f,
            Xlen::X64 => 0x3fff,
        }
    }

    /// Extracts a defined `MODE` encoding for the effective Supervisor width.
    ///
    /// Returns `None` for an unknown mode or a width exceeding the native word.
    #[inline]
    pub const fn mode_for(self, xlen: Xlen) -> Option<u8> {
        match xlen {
            Xlen::X32 => Some(((self.0 >> 31) & 1) as u8),
            Xlen::X64 => match self.0.checked_shr(60) {
                Some(mode @ (0 | 8 | 9 | 10)) => Some(mode as u8),
                _ => None,
            },
        }
    }

    /// Extracts `VMID` using the effective Supervisor register width (`HSXLEN`).
    ///
    /// Returns `None` for an unknown mode or a width exceeding the native word.
    #[inline]
    pub const fn vmid_for(self, xlen: Xlen) -> Option<usize> {
        if self.mode_for(xlen).is_none() {
            return None;
        }
        Some((self.0 >> Self::vmid_shift(xlen)) & Self::max_vmid_for(xlen))
    }

    const fn vmid_shift(xlen: Xlen) -> u32 {
        match xlen {
            Xlen::X32 => 22,
            Xlen::X64 => 44,
        }
    }

    /// Encodes a paged address space with VMID zero and the given root page number.
    ///
    /// Returns `None` if the encoding does not fit the native word, the page
    /// number exceeds the mode's field, or the root is not four-page aligned.
    #[inline]
    pub const fn paged(mode: GuestPagingMode, root_ppn: usize) -> Option<Self> {
        if root_ppn & 3 != 0 {
            return None;
        }
        match mode {
            GuestPagingMode::Sv32x4 if root_ppn < 1 << 22 => Some(Self(1 << 31 | root_ppn)),
            #[cfg(target_pointer_width = "64")]
            GuestPagingMode::Sv39x4 | GuestPagingMode::Sv48x4 | GuestPagingMode::Sv57x4
                if root_ppn < 1 << 44 =>
            {
                Some(Self((mode as usize) << 60 | root_ppn))
            }
            _ => None,
        }
    }

    /// Replaces the VMID of a paged context, preserving its mode and root.
    ///
    /// Returns `None` for Bare, an unrepresentable mode, or an out-of-range VMID.
    pub const fn with_vmid(self, vmid: usize, xlen: Xlen) -> Option<Self> {
        let max = Self::max_vmid_for(xlen);
        if !matches!(self.mode_for(xlen), Some(1 | 8..=10)) || vmid > max {
            return None;
        }
        let shift = Self::vmid_shift(xlen);
        let word = match xlen {
            Xlen::X32 => self.0 & u32::MAX as usize,
            Xlen::X64 => self.0,
        };
        Some(Self((word & !(max << shift)) | (vmid << shift)))
    }
}

define_csr! {
    /// Hypervisor guest address translation and protection ([`hgatp`](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#hgatp)).
    ///
    /// # Access requirements
    ///
    /// Preserve mappings used by active G-stage accesses.
    /// When writing Bare mode, set all other fields to zero.
    /// Changing `MODE` for a VMID requires `HFENCE.GVMA` with `rs1 = x0`
    /// and `rs2` selecting that VMID or all VMIDs, even when switching to or from Bare.
    /// Hardware may replace an unsupported `MODE` with a legal setting.
    /// See the [synchronization rules](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#norm:hfence-gvma_mode).
    Hgatp { 0x680, r: GuestAddressSpace, w: GuestAddressSpace };
}
