//! Chapter 15. Nested Acceleration Extension (EID #0x4E41434C "NACL").

/// Extension ID for Nested Acceleration Extension.
pub const EID_NACL: usize = crate::eid_from_str("NACL") as _;

pub use fid::*;

/// Declared in § 15.15.
mod fid {
    /// Function ID to probe a nested acceleration feature.
    ///
    /// Declared in §15.5.
    pub const PROBE_FEATURE: usize = 0;
    /// Function ID to set and enable the shared memory for nested acceleration on the calling hart.
    ///
    /// Declared in §15.6.
    pub const SET_SHMEM: usize = 1;
    /// Function ID to synchronize CSRs in the nested acceleration shared memory.
    ///
    /// Declared in §15.7.
    pub const SYNC_CSR: usize = 2;
    /// Function ID to synchronize HFENCEs in the nested acceleration shared memory.
    ///
    /// Declared in §15.8.
    pub const SYNC_HFENCE: usize = 3;
    /// Function ID to synchronize CSRs and HFENCEs in the nested acceleration shared memory and emulate the SRET instruction.
    ///
    /// Declared in §15.9.
    pub const SYNC_SRET: usize = 4;
}

/// Nested Acceleration Feature ID.
///
/// Declared in §15.
pub mod feature_id {
    /// Feature ID for the CSR synchronizing feature.
    ///
    /// Declared in §15.1.
    pub const SYNC_CSR: usize = 0;
    /// Feature ID for the HFENCE synchronizing feature.
    ///
    /// Declared in §15.2.
    pub const SYNC_HFENCE: usize = 1;
    /// Feature ID for the SRET synchronizing feature.
    ///
    /// Declared in §15.3.
    pub const SYNC_SRET: usize = 2;
    /// Feature ID for the auto-swap CSR feature.
    ///
    /// Declared in §15.4.
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
    pub const NATIVE: usize = 4096 + 1024 * size_of::<usize>();

    /// Size of NACL shared memory on RV32 platforms.
    pub const RV32: usize = 4096 + 1024 * size_of::<u32>();

    /// Size of NACL shared memory on RV64 platforms.
    pub const RV64: usize = 4096 + 1024 * size_of::<u64>();

    /// Size of NACL shared memory on RV128 platforms.
    pub const RV128: usize = 4096 + 1024 * size_of::<u128>();
}
