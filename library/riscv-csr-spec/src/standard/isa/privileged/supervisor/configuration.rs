//! Supervisor environment configuration.

use crate::standard::isa::values::cache_block::CacheBlockInvalidate;
#[cfg(target_pointer_width = "64")]
use crate::standard::isa::values::pointer_masking::PointerMasking;

define_csr! {
    /// Supervisor environment configuration register (`senvcfg`).
    ///
    /// # Access requirements
    ///
    /// Configure the U-mode cache-block operations and pointer masking expected by the active process.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#senvcfg>.
    Senvcfg { 0x10a, r, w } {
        FIOM[0] { rw: bool }
        LPE[2] { rw: bool }
        SSE[3] { rw: bool }
        CBIE[4..=5] { r: Result<CacheBlockInvalidate, usize>, w: CacheBlockInvalidate }
        CBCFE[6] { rw: bool }
        CBZE[7] { rw: bool }
        /// U-mode [pointer masking](https://docs.riscv.org/reference/isa/v20260120/priv/zpm.html#pointer-mask-extensions) under `Ssnpm`.
        #[cfg(target_pointer_width = "64")]
        PMM[32..=33] { r: Result<PointerMasking, usize>, w: PointerMasking }
    }
}
