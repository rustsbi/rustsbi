//! Chapter 15. Nested Acceleration Extension (EID #0x4E41434C "NACL").

/// Extension ID for Nested Acceleration Extension.
#[doc(alias = "sbi_eid_nacl")]
pub const EID_NACL: usize = crate::eid_from_str("NACL") as _;

pub use fid::*;

/// Declared in § 15.15.
mod fid {
    /// Function ID to probe a nested acceleration feature.
    ///
    /// Declared in §15.5.
    #[doc(alias = "sbi_probe_feature")]
    pub const PROBE_FEATURE: usize = 0;
    /// Function ID to set and enable the shared memory for nested acceleration on the calling hart.
    ///
    /// Declared in §15.6.
    #[doc(alias = "sbi_set_shmem")]
    pub const SET_SHMEM: usize = 1;
    /// Function ID to synchronize CSRs in the nested acceleration shared memory.
    ///
    /// Declared in §15.7.
    #[doc(alias = "sbi_sync_csr")]
    pub const SYNC_CSR: usize = 2;
    /// Function ID to synchronize HFENCEs in the nested acceleration shared memory.
    ///
    /// Declared in §15.8.
    #[doc(alias = "sbi_sync_hfence")]
    pub const SYNC_HFENCE: usize = 3;
    /// Function ID to synchronize CSRs and HFENCEs in the nested acceleration shared memory and emulate the SRET instruction.
    ///
    /// Declared in §15.9.
    #[doc(alias = "sbi_sync_sret")]
    pub const SYNC_SRET: usize = 4;
}

/// Nested Acceleration Feature ID.
///
/// Declared in §15.
pub mod feature_id {
    /// Feature ID for the CSR synchronizing feature.
    ///
    /// Declared in §15.1.
    #[doc(alias = "sbi_sync_csr")]
    pub const SYNC_CSR: usize = 0;
    /// Feature ID for the HFENCE synchronizing feature.
    ///
    /// Declared in §15.2.
    #[doc(alias = "sbi_sync_hfence")]
    pub const SYNC_HFENCE: usize = 1;
    /// Feature ID for the SRET synchronizing feature.
    ///
    /// Declared in §15.3.
    #[doc(alias = "sbi_sync_sret")]
    pub const SYNC_SRET: usize = 2;
    /// Feature ID for the auto-swap CSR feature.
    ///
    /// Declared in §15.4.
    #[doc(alias = "sbi_autoswap_csr")]
    pub const AUTOSWAP_CSR: usize = 3;
}

/// Size of shared memory set by supervisor software for current hart.
///
/// NACL shared memory includes scratch space and CSR space. Due to the difference
/// of CSR width, this size varies between different `XLEN` values. `NATIVE`
/// constant here only matches the integer width for the target this crate is compiled.
/// If you are writing an SEE with different `XLEN` from the host platform, you should
/// choose other correct constant value from `RV32`, `RV64` or `RV128` in module `shmem_size`
/// instead.
pub mod shmem_size {
    use core::mem::size_of;
    /// Size of NACL shared memory on platforms with `XLEN` of the same width as the current platform.
    #[doc(alias = "sbi_native")]
    pub const NATIVE: usize = 4096 + 1024 * size_of::<usize>();

    /// Size of NACL shared memory on RV32 platforms.
    #[doc(alias = "sbi_rv32")]
    pub const RV32: usize = 4096 + 1024 * size_of::<u32>();

    /// Size of NACL shared memory on RV64 platforms.
    #[doc(alias = "sbi_rv64")]
    pub const RV64: usize = 4096 + 1024 * size_of::<u64>();

    /// Size of NACL shared memory on RV128 platforms.
    #[doc(alias = "sbi_rv128")]
    pub const RV128: usize = 4096 + 1024 * size_of::<u128>();
}
