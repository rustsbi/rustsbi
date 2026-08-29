//! Device-power service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::DEVICE_POWER;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get the number of device-power domains.
pub const GET_NUM_DOMAINS: u8 = 0x02;
/// Get attributes of a device-power domain.
pub const GET_ATTRIBUTES: u8 = 0x03;
/// Set the state of a device-power domain.
pub const SET_STATE: u8 = 0x04;
/// Get the state of a device-power domain.
pub const GET_STATE: u8 = 0x05;

/// Mask of the state value in a power-state word.
const STATE_VALUE_MASK: u32 = 0x0000_ffff;
/// Bit indicating that context is lost in the selected state.
pub const CONTEXT_LOST: u32 = 1 << 16;
/// First vendor-specific device-power state value.
pub const VENDOR_SPECIFIC_START: u16 = 0x1000;
/// Last vendor-specific device-power state value.
pub const VENDOR_SPECIFIC_END: u16 = 0xffff;

/// A device-power state value.
#[repr(u16)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StateValue {
    /// The device is on.
    #[default]
    On = 0x0000,
    /// The device is off.
    Off = 0x0003,
}

impl StateValue {
    /// Returns the state-value encoding.
    pub const fn bits(self) -> u16 {
        self as u16
    }
}

impl TryFrom<u16> for StateValue {
    type Error = u16;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        match value {
            0x0000 => Ok(Self::On),
            0x0003 => Ok(Self::Off),
            _ => Err(value),
        }
    }
}

impl From<StateValue> for u16 {
    fn from(value: StateValue) -> Self {
        value.bits()
    }
}

/// A composite device-power state word.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct PowerState(u32);

impl PowerState {
    /// Creates a state from a value and its context-loss property.
    pub const fn new(value: StateValue, context_lost: bool) -> Self {
        Self((value.bits() as u32) | if context_lost { CONTEXT_LOST } else { 0 })
    }

    /// Creates a state from a bit representation, preserving reserved bits.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the state's bit representation.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns the state value, or its unrecognized encoding.
    pub fn value(self) -> Result<StateValue, u16> {
        StateValue::try_from((self.0 & STATE_VALUE_MASK) as u16)
    }

    /// Returns whether entering this state loses device context.
    pub const fn context_lost(self) -> bool {
        self.0 & CONTEXT_LOST != 0
    }
}
