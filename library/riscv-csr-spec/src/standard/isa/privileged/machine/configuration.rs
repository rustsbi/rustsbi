//! Machine environment configuration.

use crate::standard::isa::values::cache_block::CacheBlockInvalidate;
#[cfg(target_pointer_width = "64")]
use crate::standard::isa::values::pointer_masking::PointerMasking;

/// A Boolean machine environment field supporting atomic set and clear.
pub type MenvcfgBit<const BIT: u32> =
    crate::Field<Menvcfg, u64, bool, bool, crate::capability::SetClear, BIT, BIT>;

define_csr! {
    /// Machine environment configuration (`menvcfg`, with `menvcfgh` on RV32).
    ///
    /// Changing [`PBMTE` or `ADUE`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#sec:menvcfg)
    /// changes how page-table entries are interpreted. On this hart,
    /// `SFENCE.VMA x0, x0` synchronizes S/HS translations with the change.
    /// With H, `HFENCE.GVMA x0, x0` synchronizes guest translations.
    Menvcfg { (0x30a, 0x31a), r, w(Self | Set | Clear) } {
        FIOM[0] { rw: bool }
        LPE[2] { rw: bool, bit }
        SSE[3] { rw: bool, bit }
        CBIE[4..=5] { r: Result<CacheBlockInvalidate, usize>, w: CacheBlockInvalidate }
        CBCFE[6] { rw: bool }
        CBZE[7] { rw: bool }
        /// [Smnpm pointer masking](https://docs.riscv.org/reference/isa/v20260120/priv/zpm.html#pointer-mask-extensions) for S/HS-mode, or U-mode on a hart without S-mode.
        #[cfg(target_pointer_width = "64")]
        PMM[32..=33] { r: Result<PointerMasking, usize>, w: PointerMasking }
        /// Supervisor double-trap enable (`Ssdbltrp`).
        DTE[59] { rw: bool, bit }
        /// Hardware updates of accessed and dirty PTE bits (`Svadu`).
        ADUE[61] { rw: bool, bit }
        /// Page-based memory types enable (`Svpbmt`).
        PBMTE[62] { rw: bool }
        /// Supervisor timer comparator enable (`Sstc`).
        STCE[63] { rw: bool, bit }
    }
}
