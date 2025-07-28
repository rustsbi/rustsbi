//! Chapter 10. COVE Host Extension (EID #0x434F5648 "COVH").

/// Extension ID for COVE Host Extension.
#[doc(alias = "SBI_EXT_COVH")]
pub const EID_COVH: usize = crate::eid_from_str("COVH") as _;
pub use fid::*;

/// Declared in §10.
mod fid {
    /// Function ID to get TEE Security Monitor (TSM) information.
    ///
    /// Declared in §10.2.
    #[doc(alias = "SBI_EXT_COVH_GET_TSM_INFO")]
    pub const GET_TSM_INFO: usize = 0;
    /// Function ID to convert pages.
    ///
    /// Declared in §10.3.
    #[doc(alias = "SBI_EXT_COVH_CONVERT_PAGES")]
    pub const CONVERT_PAGES: usize = 1;
    /// Function ID to reclaim pages.
    ///
    /// Declared in §10.4.
    #[doc(alias = "SBI_EXT_COVH_RECLAIM_PAGES")]
    pub const RECLAIM_PAGES: usize = 2;
    /// Function ID to initiate global fence.
    ///
    /// Declared in §10.5.
    #[doc(alias = "SBI_EXT_COVH_GLOBAL_FENCE")]
    pub const GLOBAL_FENCE: usize = 3;
    /// Function ID to local fence.
    ///
    /// Declared in §10.6.
    #[doc(alias = "SBI_EXT_COVH_LOCAL_FENCE")]
    pub const LOCAL_FENCE: usize = 4;
    /// Function ID to create TVM.
    ///
    /// Declared in §10.7.
    #[doc(alias = "SBI_EXT_COVH_CREATE_TVM")]
    pub const CREATE_TVM: usize = 5;
    /// Function ID to finalize TVM.
    ///
    /// Declared in §10.8.
    #[doc(alias = "SBI_EXT_COVH_FINALIZE_TVM")]
    pub const FINALIZE_TVM: usize = 6;
    /// Function ID to promote to TVM.
    ///
    /// Declared in §10.9.
    #[doc(alias = "SBI_EXT_COVH_PROMOTE_TO_TVM")]
    pub const PROMOTE_TO_TVM: usize = 7;
    /// Function ID to destroy TVM.
    ///
    /// Declared in §10.10.
    #[doc(alias = "SBI_EXT_COVH_DESTROY_TVM")]
    pub const DESTROY_TVM: usize = 8;
    /// Function ID to add TVM memory region.
    ///
    /// Declared in §10.11.
    #[doc(alias = "SBI_EXT_COVH_ADD_TVM_MEMORY_REGION")]
    pub const ADD_TVM_MEMORY_REGION: usize = 9;
    /// Function ID to add TVM page table pages.
    ///
    /// Declared in §10.12.
    #[doc(alias = "SBI_EXT_COVH_ADD_TVM_PAGE_TABLE_PAGES")]
    pub const ADD_TVM_PAGE_TABLE_PAGES: usize = 10;
    /// Function ID to add TVM measured pages.
    ///
    /// Declared in §10.13.
    #[doc(alias = "SBI_EXT_COVH_ADD_TVM_MEASURED_PAGES")]
    pub const ADD_TVM_MEASURED_PAGES: usize = 11;
    /// Function ID to add TVM zero pages.
    ///
    /// Declared in §10.14.
    #[doc(alias = "SBI_EXT_COVH_ADD_TVM_ZERO_PAGES")]
    pub const ADD_TVM_ZERO_PAGES: usize = 12;
    /// Function ID to add TVM shared pages.
    ///
    /// Declared in §10.15.
    #[doc(alias = "SBI_EXT_COVH_ADD_TVM_SHARED_PAGES")]
    pub const ADD_TVM_SHARED_PAGES: usize = 13;
    /// Function ID to create TVM vCPU.
    ///
    /// Declared in §10.16.
    #[doc(alias = "SBI_EXT_COVH_CREATE_TVM_VCPU")]
    pub const CREATE_TVM_VCPU: usize = 14;
    /// Function ID to run TVM vCPU.
    ///
    /// Declared in §10.17.
    #[doc(alias = "SBI_EXT_COVH_RUN_TVM_VCPU")]
    pub const RUN_TVM_VCPU: usize = 15;
    /// Function ID to initiate TVM fence.
    ///
    /// Declared in §10.18.
    #[doc(alias = "SBI_EXT_COVH_TVM_FENCE")]
    pub const TVM_FENCE: usize = 16;
    /// Function ID to invalidate TVM pages.
    ///
    /// Declared in §10.19.
    #[doc(alias = "SBI_EXT_COVH_TVM_INVALIDATE_PAGES")]
    pub const TVM_INVALIDATE_PAGES: usize = 17;
    /// Function ID to validate TVM pages.
    ///
    /// Declared in §10.20.
    #[doc(alias = "SBI_EXT_COVH_TVM_VALIDATE_PAGES")]
    pub const TVM_VALIDATE_PAGES: usize = 18;
    /// Function ID to remove TVM pages.
    ///
    /// Declared in §10.21.
    #[doc(alias = "SBI_EXT_COVH_TVM_REMOVE_PAGES")]
    pub const TVM_REMOVE_PAGES: usize = 19;
}
