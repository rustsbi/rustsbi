//! Management-mode service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::MANAGEMENT_MODE;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get management-mode attributes.
pub const GET_ATTRIBUTES: u8 = 0x02;
/// Communicate with management-mode firmware.
pub const COMMUNICATE: u8 = 0x03;
