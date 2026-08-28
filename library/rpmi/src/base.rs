//! Base service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::BASE;

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get the implementation version.
pub const GET_IMPLEMENTATION_VERSION: u8 = 0x02;
/// Get the implementation ID.
pub const GET_IMPLEMENTATION_ID: u8 = 0x03;
/// Get the implemented RPMI specification version.
pub const GET_SPEC_VERSION: u8 = 0x04;
/// Get platform information.
pub const GET_PLATFORM_INFO: u8 = 0x05;
/// Probe a service group.
pub const PROBE_SERVICE_GROUP: u8 = 0x06;
/// Get base service-group attributes.
pub const GET_ATTRIBUTES: u8 = 0x07;

/// Base service-group event IDs.
pub mod event {
    /// The platform controller is unable to serve message requests reliably.
    pub const REQUEST_HANDLE_ERROR: u8 = 0x01;
}

/// The complete base attribute flags word.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attributes(u32);

impl Attributes {
    const EVENT_NOTIFICATION_SUPPORT_MASK: u32 = 1 << 0;
    const CONTEXT_M_MODE_MASK: u32 = 1 << 1;
    const RESERVED_MASK: u32 = !(Self::EVENT_NOTIFICATION_SUPPORT_MASK | Self::CONTEXT_M_MODE_MASK);

    /// Creates an attribute word with reserved bits cleared.
    pub const fn new(event_notification_supported: bool, context_m_mode: bool) -> Self {
        Self(
            if event_notification_supported {
                Self::EVENT_NOTIFICATION_SUPPORT_MASK
            } else {
                0
            } | if context_m_mode {
                Self::CONTEXT_M_MODE_MASK
            } else {
                0
            },
        )
    }

    /// Creates attributes from a bit representation without validation.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the attributes' bit representation.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns whether event notifications are supported.
    pub const fn event_notification_supported(self) -> bool {
        self.0 & Self::EVENT_NOTIFICATION_SUPPORT_MASK != 0
    }

    /// Returns whether the RPMI context privilege level is M-mode.
    ///
    /// When false, the context privilege level is S-mode.
    pub const fn context_m_mode(self) -> bool {
        self.0 & Self::CONTEXT_M_MODE_MASK != 0
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}

/// Implementation IDs assigned by the RPMI specification.
pub mod implementation_id {
    /// The reference `librpmi` implementation.
    pub const LIBRPMI: u32 = 0;
}

/// RISC-V RPMI specification version 1.0, ratified at Jul 16, 2025.
pub const SPEC_VERSION: crate::Version = crate::Version::new(1, 0);
/// Base service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);
