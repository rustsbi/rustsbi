//! Chapter 12. COVE Guest Extension (EID #0x434F5647 "COVG").

/// Extension ID for COVE Guest Extension.
#[doc(alias = "SBI_EXT_COVG")]
pub const EID_COVG: usize = crate::eid_from_str("COVG") as _;
pub use fid::*;

/// Declared in §12.
mod fid {
    /// Function ID to mark the specified range of TVM physical address space as used for emulated MMIO.
    ///
    /// Declared in §12.1.
    #[doc(alias = "SBI_EXT_COVG_ADD_MMIO_REGION")]
    pub const ADD_MMIO_REGION: usize = 0;
    /// Function ID to remove the specified range of TVM physical address space from the emulated MMIO regions.
    ///
    /// Declared in §12.2.
    #[doc(alias = "SBI_EXT_COVG_REMOVE_MMIO_REGION")]
    pub const REMOVE_MMIO_REGION: usize = 1;
    /// Function ID to initiate the assignment-change of TVM physical address space from confidential to non-confidential/shared memory.
    ///
    /// Declared in §12.3.
    #[doc(alias = "SBI_EXT_COVG_SHARE_MEMORY_REGION")]
    pub const SHARE_MEMORY_REGION: usize = 2;
    /// Function ID to initiate the assignment-change of TVM physical address space from shared to confidential.
    ///
    /// Declared in §12.4.
    #[doc(alias = "SBI_EXT_COVG_UNSHARE_MEMORY_REGION")]
    pub const UNSHARE_MEMORY_REGION: usize = 3;
    /// Function ID to allow injection of the specified external interrupt ID into the calling TVM vCPU.
    ///
    /// Declared in §12.5.
    #[doc(alias = "SBI_EXT_COVG_ALLOW_EXTERNAL_INTERRUPT")]
    pub const ALLOW_EXTERNAL_INTERRUPT: usize = 4;
    /// Function ID to deny injection of the specified external interrupt ID into the calling TVM vCPU.
    ///
    /// Declared in §12.6.
    #[doc(alias = "SBI_EXT_COVG_DENY_EXTERNAL_INTERRUPT")]
    pub const DENY_EXTERNAL_INTERRUPT: usize = 5;
    /// Function ID to get the SBI implementation attestation capabilities.
    ///
    /// Declared in §12.7.
    #[doc(alias = "SBI_EXT_COVG_GET_ATTESTATION_CAPABILITIES")]
    pub const GET_ATTESTATION_CAPABILITIES: usize = 6;
    /// Function ID to extend the TVM runtime set of measurements with one additional data blob.
    ///
    /// Declared in §12.8.
    #[doc(alias = "SBI_EXT_COVG_EXTEND_MEASUREMENT")]
    pub const EXTEND_MEASUREMENT: usize = 7;
    /// Function ID to get an attestation evidence to report to a remote relying party.
    ///
    /// Declared in §12.9.
    #[doc(alias = "SBI_EXT_COVG_GET_EVIDENCE")]
    pub const GET_EVIDENCE: usize = 8;
    /// Function ID to request TSM for a secret available after successful local attestation.
    ///
    /// Declared in §12.10.
    #[doc(alias = "SBI_EXT_COVG_RETRIEVE_SECRET")]
    pub const RETRIEVE_SECRET: usize = 9;
    /// Function ID to return a TVM measurement register value for the specified measurement register.
    ///
    /// Declared in §12.11.
    #[doc(alias = "SBI_EXT_COVG_READ_MEASUREMENT")]
    pub const READ_MEASUREMENT: usize = 10;
}
