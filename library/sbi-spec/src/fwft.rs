//! Chapter 18. Firmware Features Extension (EID #0x46574654 "FWFT").

/// Extension ID for Firmware Features Extension.
#[doc(alias = "sbi_eid_fwft")]
pub const EID_FWFT: usize = crate::eid_from_str("FWFT") as _;
pub use fid::*;

/// Declared in §18.3.
mod fid {
    /// Set the firmware function of the request based on Value and Flags parameters.
    ///
    /// Declared in §18.1.
    #[doc(alias = "sbi_set")]
    pub const SET: usize = 0;
    /// Return to the firmware function configuration value.
    ///
    /// Declared in §18.2.
    #[doc(alias = "sbi_get")]
    pub const GET: usize = 1;
}

/// FWFT Feature Types.
///
/// Declared in §18.
pub mod feature_type {
    /// Control misaligned access exception delegation.
    ///
    /// Declared in §18.
    #[doc(alias = "sbi_misaligned_exc_deleg")]
    pub const MISALIGNED_EXC_DELEG: usize = 0;
    /// Control landing pad support.
    ///
    /// Declared in §18.
    #[doc(alias = "sbi_landing_pad")]
    pub const LANDING_PAD: usize = 1;
    /// Control shadow stack support.
    ///
    /// Declared in §18.
    #[doc(alias = "sbi_shadow_stack")]
    pub const SHADOW_STACK: usize = 2;
    /// Control double trap support.
    ///
    /// Declared in §18.
    #[doc(alias = "sbi_double_trap")]
    pub const DOUBLE_TRAP: usize = 3;
    /// Control hardware updating of PTE A/D bits.
    ///
    /// Declared in §18.
    #[doc(alias = "sbi_pte_ad_hw_updating")]
    pub const PTE_AD_HW_UPDATING: usize = 4;
    /// Control the pointer masking tag length.
    ///
    /// Declared in §18.
    #[doc(alias = "sbi_pointer_masking_pmlen")]
    pub const POINTER_MASKING_PMLEN: usize = 5;
}

/// Firmware Features Set.
///
/// Declared in §18.1.
pub mod flags {
    use bitflags::bitflags;

    bitflags! {
        #[derive(Clone, Copy, PartialEq, Eq)]
        /// Declared in Table 94.
        pub struct SetFlags: usize {
            /// If provided, once set, the feature value can no longer be modified.
            #[doc(alias = "sbi_lock")]
            const LOCK = 1 << 0;
        }
    }
}
