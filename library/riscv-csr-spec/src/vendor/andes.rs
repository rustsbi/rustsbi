//! Andes and Allwinner V821 custom CSRs.
//!
//! The machine registers require M-mode on a hart implementing the selected
//! register. On AX45MP the user CCTL ports also admit S/U accesses when
//! `mcache_ctl.CCTL_SUEN` is set, subject to the command's privilege restriction.
//! PMA writes require ownership of the affected entry and coordination of memory
//! attributes, cache state, and active memory accesses.
//!
//! Sources:
//!
//! - [AndesCore AX45MP-1C Processor Reference Manual UM248, v1.2, CPU revision 5.0.0](https://www.andestech.com/wp-content/uploads/AX45MP-1C-Rev.-5.0.0-Datasheet.pdf):
//!   cache control (§16.12), address-based cache operations (§16.12.14), and PMA (§16.17).
//! - [HPM6E00 User Manual](https://www.hpmicro.com/Public/Uploads/uploadfile/files/20250109/HPM6E00UMV06.pdf),
//!   §§3.2.119–120 describe the RV32 PMA configuration and address registers.
//! - [Allwinner V821 BSP, `ads_rv32/mmu.c`](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/spl/arch/riscv/cpu/ads_rv32/mmu.c):
//!   V821 use of `McacheCtl`, `Ucctlbeginaddr`, and `Ucctlcommand`.

field_value! {
    /// One 8-bit Andes PMA entry configuration byte.
    pub struct PmaEntryConfig(u8);
}

impl PmaEntryConfig {
    /// Bufferable, non-cacheable NAPOT memory with AMOs permitted.
    pub const NONCACHEABLE_NAPOT: Self = Self(0x0f);
}

csr_value! {
    /// An Andes PMA address-register value. In NAPOT mode its low bits encode the range size.
    pub struct PmaAddress;
}

impl PmaAddress {
    /// Encodes the NAPOT range starting at `base`.
    ///
    /// Returns `None` unless `size` is a power of two of at least 4 KiB,
    /// `base` is aligned to `size`, the exclusive end fits in `u64`, and
    /// the resulting register value fits in `usize`.
    #[inline(always)]
    pub const fn napot(base: u64, size: u64) -> Option<Self> {
        if size < 4096
            || !size.is_power_of_two()
            || (base & (size - 1)) != 0
            || base.checked_add(size).is_none()
        {
            return None;
        }
        let encoded = (base >> 2) | (size / 8 - 1);
        if encoded > usize::MAX as u64 {
            return None;
        }
        Some(Self(encoded as usize))
    }
}

/// Address-based L1 data-cache maintenance commands (Andes AX45MP manual §16.12.14).
#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CacheLineOperation {
    /// Invalidates the addressed cache line.
    Invalidate = 0,
    /// Writes back the addressed cache line.
    WriteBack = 1,
    /// Writes back and invalidates the addressed cache line.
    WriteBackAndInvalidate = 2,
}

define_csr! {
    /// Machine cache control.
    McacheCtl { 0x7ca, r: usize };

    /// Machine miscellaneous control (AX45MP-1C §16.12.7).
    MmiscCtl { 0x7d0, r: usize };

    /// RV32 PMA configuration for entries 12 through 15.
    ///
    /// # Access requirements
    ///
    /// Preserve entries `12..=14` when updating entry 15, and synchronize
    /// memory and cache state.
    only32 Pmacfg3 { 0xbc3, r, w } {
        /// PMA entry 15 configuration.
        ENTRY15[24..=31] { rw: PmaEntryConfig }
    }

    /// PMA entry 15 encoded address.
    ///
    /// # Access requirements
    ///
    /// Supply a valid, supported range for entry 15.
    Pmaaddr15 { 0xbdf, r: PmaAddress, w: PmaAddress };

    /// Operand address for cache-control commands.
    Ucctlbeginaddr { 0x80b };

    /// Cache-control command port.
    Ucctlcommand { 0x80c };
}

impl Ucctlcommand {
    /// Command encoding for writing back and invalidating all L1 data-cache lines.
    pub const L1D_WBINVAL_ALL: usize = 6;
}
