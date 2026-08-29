//! Request-forward service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::REQUEST_FORWARD;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Retrieve the current forwarded request message.
pub const RETRIEVE_CURRENT_MESSAGE: u8 = 0x02;
/// Complete the current forwarded request message.
pub const COMPLETE_CURRENT_MESSAGE: u8 = 0x03;

/// Request-forward event IDs.
pub mod event {
    /// A new forwarded request is available.
    pub const NEW_MESSAGE: u8 = 0x01;
}
