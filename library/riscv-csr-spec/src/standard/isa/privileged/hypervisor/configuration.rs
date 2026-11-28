//! Hypervisor environment configuration.

use crate::standard::isa::values::cache_block::CacheBlockInvalidate;
#[cfg(target_pointer_width = "64")]
use crate::standard::isa::values::pointer_masking::PointerMasking;

define_csr! {
    /// Unified 64-bit hypervisor environment configuration (`henvcfg` / RV32 `henvcfgh`).
    ///
    /// # Access requirements
    ///
    /// Coordinate changes with guest execution. After changing `PBMTE` or `ADUE`,
    /// `HFENCE.VVMA x0, x0` synchronizes VS-stage translations for the current VMID.
    ///
    /// See the [environment controls](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#henvcfg)
    /// and [two-stage translation rules](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#two-stage-translation).
    Henvcfg { (0x60a, 0x61a), r, w } {
        FIOM[0] { rw: bool }
        LPE[2] { rw: bool }
        SSE[3] { rw: bool }
        CBIE[4..=5] { r: Result<CacheBlockInvalidate, usize>, w: CacheBlockInvalidate }
        CBCFE[6] { rw: bool }
        CBZE[7] { rw: bool }
        /// [Ssnpm pointer masking](https://docs.riscv.org/reference/isa/v20260120/priv/zpm.html#pointer-mask-extensions) for VS-mode. VU-mode uses `senvcfg.PMM`.
        #[cfg(target_pointer_width = "64")]
        PMM[32..=33] { r: Result<PointerMasking, usize>, w: PointerMasking }
        /// Guest double-trap enable (`Ssdbltrp`).
        DTE[59] { rw: bool }
        /// Hardware updates of accessed and dirty bits in VS-stage PTEs (`Svadu`).
        ADUE[61] { rw: bool }
        /// VS-stage page-based memory types enable (`Svpbmt`).
        PBMTE[62] { rw: bool }
        /// VS-mode timer comparator enable ([Sstc](https://docs.riscv.org/reference/isa/v20260120/priv/sstc.html#Sstc)).
        STCE[63] { rw: bool }
    }
}
