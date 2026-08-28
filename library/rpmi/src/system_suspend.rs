//! System-suspend service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::SYSTEM_SUSPEND;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get attributes for a suspend type.
pub const GET_ATTRIBUTES: u8 = 0x02;
/// Suspend the system.
pub const SUSPEND: u8 = 0x03;

/// Suspend-type values shared with the SBI System Suspend extension.
pub mod suspend_type {
    /// Suspend to RAM.
    pub const SUSPEND_TO_RAM: u32 = 0;
}

/// Suspend-type attribute flags.
pub mod attribute {
    /// The suspend type is supported.
    pub const SUPPORTED: u32 = 1 << 0;
    /// A caller-provided resume address is supported.
    pub const CUSTOM_RESUME_ADDRESS: u32 = 1 << 1;
}
