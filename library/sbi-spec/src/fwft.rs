//! Chapter 18. Firmware Features Extension (EID #0x46574654 "FWFT").

/// Extension ID for Firmware Features Extension.
#[doc(alias = "SBI_EXT_FWFT")]
pub const EID_FWFT: usize = crate::eid_from_str("FWFT") as _;
pub use fid::*;

/// Declared in §18.3.
mod fid {
    /// Set the firmware function of the request based on Value and Flags parameters.
    ///
    /// Declared in §18.1.
    #[doc(alias = "SBI_EXT_FWFT_SET")]
    pub const SET: usize = 0;
    /// Return to the firmware function configuration value.
    ///
    /// Declared in §18.2.
    #[doc(alias = "SBI_EXT_FWFT_GET")]
    pub const GET: usize = 1;
}

/// FWFT Feature Types.
///
/// Declared in §18.
pub mod feature_type {
    /// Control misaligned access exception delegation.
    ///
    /// Declared in §18.
    pub const MISALIGNED_EXC_DELEG: usize = 0;
    /// Control landing pad support.
    ///
    /// Declared in §18.
    pub const LANDING_PAD: usize = 1;
    /// Control shadow stack support.
    ///
    /// Declared in §18.
    pub const SHADOW_STACK: usize = 2;
    /// Control double trap support.
    ///
    /// Declared in §18.
    pub const DOUBLE_TRAP: usize = 3;
    /// Control hardware updating of PTE A/D bits.
    ///
    /// Declared in §18.
    pub const PTE_AD_HW_UPDATING: usize = 4;
    /// Control the pointer masking tag length.
    ///
    /// Declared in §18.
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
            const LOCK = 1 << 0;
        }
    }
}
