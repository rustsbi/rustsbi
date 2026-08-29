//! Voltage service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::VOLTAGE;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get the number of voltage domains.
pub const GET_NUM_DOMAINS: u8 = 0x02;
/// Get attributes of a voltage domain.
pub const GET_ATTRIBUTES: u8 = 0x03;
/// Get supported voltage levels.
pub const GET_SUPPORTED_LEVELS: u8 = 0x04;
/// Set voltage-domain configuration.
pub const SET_CONFIG: u8 = 0x05;
/// Get voltage-domain configuration.
pub const GET_CONFIG: u8 = 0x06;
/// Set a signed 32-bit voltage level in microvolts.
pub const SET_LEVEL: u8 = 0x07;
/// Get a signed 32-bit voltage level in microvolts.
pub const GET_LEVEL: u8 = 0x08;

/// A voltage-level format.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LevelFormat {
    /// Voltage levels are listed individually.
    #[default]
    Discrete = 0,
    /// Voltage levels are described by linear ranges.
    LinearRange = 1,
}

impl LevelFormat {
    /// Returns the unshifted format encoding.
    pub const fn bits(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for LevelFormat {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Discrete),
            1 => Ok(Self::LinearRange),
            _ => Err(value),
        }
    }
}

impl From<LevelFormat> for u8 {
    fn from(value: LevelFormat) -> Self {
        value.bits()
    }
}

/// Voltage-domain attributes returned by [`GET_ATTRIBUTES`].
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attributes(u32);

impl Attributes {
    const ALWAYS_ON_MASK: u32 = 1 << 0;
    const FORMAT_SHIFT: u32 = 1;
    const FORMAT_MASK: u32 = 0b111 << Self::FORMAT_SHIFT;
    const RESERVED_MASK: u32 = !(Self::ALWAYS_ON_MASK | Self::FORMAT_MASK);

    /// Creates attributes with reserved bits cleared.
    pub const fn new(level_format: LevelFormat, always_on: bool) -> Self {
        Self(
            ((level_format.bits() as u32) << Self::FORMAT_SHIFT)
                | if always_on { Self::ALWAYS_ON_MASK } else { 0 },
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

    /// Returns whether the voltage domain is always on.
    pub const fn always_on(self) -> bool {
        self.0 & Self::ALWAYS_ON_MASK != 0
    }

    /// Returns the voltage-level format, or its unrecognized encoding.
    pub fn level_format(self) -> Result<LevelFormat, u8> {
        LevelFormat::try_from(((self.0 & Self::FORMAT_MASK) >> Self::FORMAT_SHIFT) as u8)
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}

/// The complete voltage-domain configuration word.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ConfigFlags(u32);

impl ConfigFlags {
    const ENABLED_MASK: u32 = 1 << 0;
    const RESERVED_MASK: u32 = !Self::ENABLED_MASK;

    /// Creates a configuration word with reserved bits cleared.
    pub const fn new(enabled: bool) -> Self {
        Self(if enabled { Self::ENABLED_MASK } else { 0 })
    }

    /// Creates a configuration word from its bit representation without validation.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the configuration word's bit representation.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns whether the voltage domain is enabled.
    pub const fn enabled(self) -> bool {
        self.0 & Self::ENABLED_MASK != 0
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}

/// A linear voltage range in microvolts.
///
/// The fields are logical RPMI words, not transport-serialized bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LinearRange {
    /// Minimum voltage in microvolts.
    pub min_uv: u32,
    /// Maximum voltage in microvolts.
    pub max_uv: u32,
    /// Voltage step in microvolts.
    pub step_uv: u32,
}
