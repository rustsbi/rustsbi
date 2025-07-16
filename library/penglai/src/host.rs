//! Penglai PMP host-side extension (Penglai Host extension) spec.

/// Extension ID for Penglai Host extension.
///
/// Penglai Host extension isn't a standard extension. The currently used extension ID is temporary.
pub const EID_PENGLAI_HOST: usize = 0x100100;
pub use fid::*;

mod fid {
    /// Feature ID for init secure memory management.
    #[doc(alias = "SBI_MM_INIT")]
    pub const MM_INIT: usize = 100;
    /// Feature ID for create an enclave.
    #[doc(alias = "SBI_CREATE_ENCLAVE")]
    pub const CREATE_ENCLAVE: usize = 99;
    /// Feature ID for attest enclave and generate attest report.
    #[doc(alias = "SBI_ATTEST_ENCLAVE")]
    pub const ATTEST_ENCLAVE: usize = 98;
    /// Feature ID for running enclave on current hart.
    #[doc(alias = "SBI_RUN_ENCLAVE")]
    pub const RUN_ENCLAVE: usize = 97;
    /// Feature ID for stoping enclave.
    #[doc(alias = "SBI_STOP_ENCLAVE")]
    pub const STOP_ENCLAVE: usize = 96;
    /// Feature ID for resume enclave.
    #[doc(alias = "SBI_RESUME_ENCLAVE")]
    pub const RESUME_ENCLAVE: usize = 95;
    /// Feature ID for destory enclave.
    #[doc(alias = "SBI_DESTROY_ENCLAVE")]
    pub const DESTROY_ENCLAVE: usize = 94;
    /// Feature ID for allocate secure memory from secure monitor.
    #[doc(alias = "SBI_ALLOC_ENCLAVE_MM")]
    pub const ALLOC_ENCLAVE_MM: usize = 93;
    /// Feature ID for extend secure memory.
    #[doc(alias = "SBI_MEMORY_EXTEND")]
    pub const MEMORY_EXTEND: usize = 92;
    /// Feature ID for reclaim secure memory from secure monitor.
    #[doc(alias = "SBI_MEMORY_RECLAIM")]
    pub const MEMORY_RECLAIM: usize = 91;
    /// Feature ID for free secure memory used by enclave.
    #[doc(alias = "SBI_FREE_ENCLAVE_MEM")]
    pub const FREE_ENCLAVE_MEM: usize = 90;
    /// Feature ID for print debug information.
    #[doc(alias = "SBI_DEBUG_PRINT")]
    pub const DEBUG_PRINT: usize = 88;
}

/// Enclave resume status.
pub mod resume_status {
    /// Resume enclave from the timer interrupt.
    pub const RESUME_FROM_TIMER_IRQ: usize = 2000;
    /// Resume enclave from enclave stopped.
    pub const RESUME_FROM_STOP: usize = 2003;
    /// Resume enclave from an ocall.
    pub const RESUME_FROM_OCALL: usize = 2;
}
