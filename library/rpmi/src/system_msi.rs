//! System-MSI service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::SYSTEM_MSI;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get system-MSI attributes.
pub const GET_ATTRIBUTES: u8 = 0x02;
/// Get attributes of one system MSI.
pub const GET_MSI_ATTRIBUTES: u8 = 0x03;
/// Set the state of one system MSI.
pub const SET_MSI_STATE: u8 = 0x04;
/// Get the state of one system MSI.
pub const GET_MSI_STATE: u8 = 0x05;
/// Set the target of one system MSI.
pub const SET_MSI_TARGET: u8 = 0x06;
/// Get the target of one system MSI.
pub const GET_MSI_TARGET: u8 = 0x07;
/// Required alignment of a system-MSI target address in bytes.
pub const TARGET_ADDRESS_ALIGNMENT: usize = 4;

/// A complete system-MSI state word.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct State(u32);

impl State {
    const ENABLED_MASK: u32 = 1 << 0;
    const PENDING_MASK: u32 = 1 << 1;
    const RESERVED_MASK: u32 = !(Self::ENABLED_MASK | Self::PENDING_MASK);

    /// Creates a state word for enabling or disabling a system MSI.
    ///
    /// The read-only pending bit and all reserved bits are cleared.
    pub const fn new(enabled: bool) -> Self {
        Self(if enabled { Self::ENABLED_MASK } else { 0 })
    }

    /// Creates a state word from its bit representation without validation.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the state word's bit representation.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns whether the system MSI is enabled.
    pub const fn enabled(self) -> bool {
        self.0 & Self::ENABLED_MASK != 0
    }

    /// Returns whether the system MSI is pending.
    pub const fn pending(self) -> bool {
        self.0 & Self::PENDING_MASK != 0
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}

/// The complete per-MSI attribute flags word.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attributes(u32);

impl Attributes {
    const PREFERRED_M_MODE_MASK: u32 = 1 << 0;
    const RESERVED_MASK: u32 = !Self::PREFERRED_M_MODE_MASK;

    /// Creates an attribute word with reserved bits cleared.
    pub const fn new(preferred_m_mode: bool) -> Self {
        Self(if preferred_m_mode {
            Self::PREFERRED_M_MODE_MASK
        } else {
            0
        })
    }

    /// Creates attributes from a bit representation without validation.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the attributes' bit representation.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns whether M-mode is preferred for handling the system MSI.
    ///
    /// When false, either M-mode or S-mode may handle it.
    pub const fn preferred_m_mode(self) -> bool {
        self.0 & Self::PREFERRED_M_MODE_MASK != 0
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}
