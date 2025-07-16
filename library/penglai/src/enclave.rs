//! Penglai PMP enclave-side extension (Penglai Enclave extension) spec.

/// Extension ID for Penglai Host extension.
///
/// Penglai Enclave extension isn't a standard extension. The currently used extension ID is temporary.
pub const EID_PENGLAI_ENCLAVE: usize = 0x100101;
pub use fid::*;

mod fid {
    /// Feature ID for enclave exit.
    #[doc(alias = "SBI_EXIT_ENCLAVE")]
    pub const ENCLAVE_EXIT: usize = 99;
    /// Feature ID for request service from host.
    #[doc(alias = "SBI_ENCLAVE_OCALL")]
    pub const ENCLAVE_OCALL: usize = 98;
    /// Feature ID for get key from secure monitor.
    #[doc(alias = "SBI_GET_KEY")]
    pub const GET_KEY: usize = 88;
}

pub mod ocall_type {
    /// ocall for request host for print.
    pub const OCALL_SYS_WRITE: usize = 3;
    /// ocall reserved for user defined.
    pub const OCALL_USER_DEFINED: usize = 9;
}
