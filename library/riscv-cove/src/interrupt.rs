//! Chapter 11. COVE Interrupt Extension (EID #0x434F5649 "COVI").

/// Extension ID for COVE Interrupt Extension.
#[doc(alias = "SBI_EXT_COVI")]
pub const EID_COVI: usize = crate::eid_from_str("COVI") as _;
pub use fid::*;

/// Declared in §11.
mod fid {
    /// Function ID to initialize TVM AIA.
    ///
    /// Declared in §11.1.
    #[doc(alias = "SBI_EXT_COVI_INIT_TVM_AIA")]
    pub const INIT_TVM_AIA: usize = 0;
    /// Function ID to set the guest physical address of the specified vCPU's virtualized IMSIC.
    ///
    /// Declared in §11.2.
    #[doc(alias = "SBI_EXT_COVI_SET_TVM_AIA_CPU_IMSIC_ADDR")]
    pub const SET_TVM_AIA_CPU_IMSIC_ADDR: usize = 1;
    /// Function ID to convert the non-confidential guest interrupt file for use with a TVM.
    ///
    /// Declared in §11.3.
    #[doc(alias = "SBI_EXT_COVI_CONVERT_AIA_IMSIC")]
    pub const CONVERT_AIA_IMSIC: usize = 2;
    /// Function ID to reclaim the confidential TVM interrupt file.
    ///
    /// Declared in §11.4.
    #[doc(alias = "SBI_EXT_COVI_RECLAIM_TVM_AIA_IMSIC")]
    pub const RECLAIM_TVM_AIA_IMSIC: usize = 3;
    /// Function ID to bind a TVM vCPU to the current physical CPU.
    ///
    /// Declared in §11.5.
    #[doc(alias = "SBI_EXT_COVI_BIND_AIA_IMSIC")]
    pub const BIND_AIA_IMSIC: usize = 4;
    /// Function ID to begin the unbinding process for the specified vCPU from its guest interrupt files.
    ///
    /// Declared in §11.6.
    #[doc(alias = "SBI_EXT_COVI_UNBIND_AIA_IMSIC_BEGIN")]
    pub const UNBIND_AIA_IMSIC_BEGIN: usize = 5;
    /// Function ID to complete the unbinding process for the specified vCPU from its guest interrupt files.
    ///
    /// Declared in §11.7.
    #[doc(alias = "SBI_EXT_COVI_UNBIND_AIA_IMSIC_END")]
    pub const UNBIND_AIA_IMSIC_END: usize = 6;
    /// Function ID to inject an external interrupt into the specified vCPU.
    ///
    /// Declared in §11.8.
    #[doc(alias = "SBI_EXT_COVI_INJECT_TVM_CPU")]
    pub const INJECT_TVM_CPU: usize = 7;
    /// Function ID to begin the rebinding process for the specified vCPU to the current physical CPU.
    ///
    /// Declared in §11.9.
    #[doc(alias = "SBI_EXT_COVI_REBIND_AIA_IMSIC_BEGIN")]
    pub const REBIND_AIA_IMSIC_BEGIN: usize = 8;
    /// Function ID to clone the old guest interrupt file of the specified vCPU.
    ///
    /// Declared in §11.10.
    #[doc(alias = "SBI_EXT_COVI_REBIND_AIA_IMSIC_CLONE")]
    pub const REBIND_AIA_IMSIC_CLONE: usize = 9;
    /// Function ID to complete the rebinding process for the specified vCPU.
    ///
    /// Declared in §11.11.
    #[doc(alias = "SBI_EXT_COVI_REBIND_AIA_IMSIC_END")]
    pub const REBIND_AIA_IMSIC_END: usize = 10;
}
