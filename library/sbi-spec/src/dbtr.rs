//! Chapter 19. Debug Triggers Extension (EID #0x44425452 "DBTR")

/// Extension ID for Debug Triggers Extension.
#[doc(alias = "SBI_EXT_DBTR")]
pub const EID_DBTR: usize = crate::eid_from_str("DBTR") as _;
pub use fid::*;

/// Declared in §19.9.
mod fid {
    /// Function ID to get the number of debug triggers on the calling hart.
    ///
    /// Declared in §19.1.
    #[doc(alias = "SBI_EXT_DBTR_NUM_TRIGGERS")]
    pub const NUM_TRIGGERS: usize = 0;
    /// Function ID to set and enable the shared memory for debug trigger configuration on the calling hart.
    ///
    /// Declared in §19.2.
    #[doc(alias = "SBI_EXT_DBTR_SET_SHMEM")]
    pub const SET_SHMEM: usize = 1;
    /// Function ID to read the debug trigger state and configuration into shared memory.
    ///
    /// Declared in §19.3.
    #[doc(alias = "SBI_EXT_DBTR_TRIGGER_READ")]
    pub const READ_TRIGGERS: usize = 2;
    /// Function ID to install debug triggers based on an array of trigger configurations.
    ///
    /// Declared in §19.4.
    #[doc(alias = "SBI_EXT_DBTR_TRIGGER_INSTALL")]
    pub const INSTALL_TRIGGERS: usize = 3;
    /// Function ID to update already installed debug triggers based on a trigger configuration array.
    ///
    /// Declared in 19.5.
    #[doc(alias = "SBI_EXT_DBTR_TRIGGER_UPDATE")]
    pub const UPDATE_TRIGGERS: usize = 4;
    /// Function ID to uninstall a set of debug triggers.
    ///
    /// Declared in 19.6.
    #[doc(alias = "SBI_EXT_DBTR_TRIGGER_UNINSTALL")]
    pub const UNINSTALL_TRIGGERS: usize = 5;
    /// Function ID to enable a set of debug triggers.
    ///
    /// Declared in 19.7.
    #[doc(alias = "SBI_EXT_DBTR_TRIGGER_ENABLE")]
    pub const ENABLE_TRIGGERS: usize = 6;
    /// Function ID to disable a set of debug triggers.
    ///
    /// Declared in 19.8.
    #[doc(alias = "SBI_EXT_DBTR_TRIGGER_DISABLE")]
    pub const DISABLE_TRIGGERS: usize = 7;
}
