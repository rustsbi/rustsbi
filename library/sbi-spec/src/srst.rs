//! Chapter 10. System Reset Extension (EID #0x53525354 "SRST").

/// Extension ID for System Reset extension.
#[doc(alias = "sbi_eid_srst")]
pub const EID_SRST: usize = crate::eid_from_str("SRST") as _;
pub use fid::*;

/// Shutdown as a reset type.
#[doc(alias = "sbi_reset_type_shutdown")]
pub const RESET_TYPE_SHUTDOWN: u32 = 0;
/// Cold Reboot as a reset type.
#[doc(alias = "sbi_reset_type_cold_reboot")]
pub const RESET_TYPE_COLD_REBOOT: u32 = 1;
/// Warm Reboot as a reset type.
#[doc(alias = "sbi_reset_type_warm_reboot")]
pub const RESET_TYPE_WARM_REBOOT: u32 = 2;

/// No Reason as reset reason.
#[doc(alias = "sbi_reset_reason_no_reason")]
pub const RESET_REASON_NO_REASON: u32 = 0;
/// System Failure as reset reason.
#[doc(alias = "sbi_reset_reason_system_failure")]
pub const RESET_REASON_SYSTEM_FAILURE: u32 = 1;

/// Declared in §10.2.
mod fid {
    /// Function ID to reset the system based on provided reset type and reason.
    ///
    /// Declared in §10.1.
    #[doc(alias = "sbi_system_reset")]
    pub const SYSTEM_RESET: usize = 0;
}
