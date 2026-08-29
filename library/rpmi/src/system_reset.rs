//! System-reset service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::SYSTEM_RESET;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get attributes for a reset type.
pub const GET_ATTRIBUTES: u8 = 0x02;
/// Perform a system reset as a posted request.
pub const RESET: u8 = 0x03;

/// Reset-type values shared with the SBI System Reset extension.
pub mod reset_type {
    /// Shut down the system.
    pub const SHUTDOWN: u32 = 0;
    /// Cold reboot the system.
    pub const COLD_REBOOT: u32 = 1;
    /// Warm reboot the system.
    pub const WARM_REBOOT: u32 = 2;
    /// First vendor- or platform-specific reset type.
    pub const VENDOR_SPECIFIC_START: u32 = 0xf000_0000;
    /// Last vendor- or platform-specific reset type.
    pub const VENDOR_SPECIFIC_END: u32 = 0xffff_ffff;
}

/// Reset-type attribute flags.
pub mod attribute {
    /// The reset type is supported.
    pub const SUPPORTED: u32 = 1 << 0;
}
