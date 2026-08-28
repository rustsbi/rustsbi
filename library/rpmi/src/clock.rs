//! Clock service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::CLOCK;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get the number of clocks.
pub const GET_NUM_CLOCKS: u8 = 0x02;
/// Get attributes of a clock.
pub const GET_ATTRIBUTES: u8 = 0x03;
/// Get supported clock rates.
pub const GET_SUPPORTED_RATES: u8 = 0x04;
/// Set clock configuration.
pub const SET_CONFIG: u8 = 0x05;
/// Get clock configuration.
pub const GET_CONFIG: u8 = 0x06;
/// Set a clock rate.
pub const SET_RATE: u8 = 0x07;
/// Get a clock rate.
pub const GET_RATE: u8 = 0x08;

/// A supported clock-rate format returned by [`GET_ATTRIBUTES`].
#[repr(u32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RateFormat {
    /// Supported rates are listed individually.
    #[default]
    Discrete = 0,
    /// Supported rates are described by linear ranges.
    LinearRange = 1,
}

impl RateFormat {
    /// Returns the attribute-word encoding.
    pub const fn bits(self) -> u32 {
        self as u32
    }
}

impl TryFrom<u32> for RateFormat {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Discrete),
            1 => Ok(Self::LinearRange),
            _ => Err(value),
        }
    }
}

impl From<RateFormat> for u32 {
    fn from(value: RateFormat) -> Self {
        value.bits()
    }
}

/// The complete clock configuration word.
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

    /// Returns whether the clock is enabled.
    pub const fn enabled(self) -> bool {
        self.0 & Self::ENABLED_MASK != 0
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}

/// A rounding mode accepted by [`SET_RATE`].
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RoundingMode {
    /// Round down to a supported rate.
    #[default]
    Down = 0,
    /// Round up to a supported rate.
    Up = 1,
    /// Let the platform choose the nearest supported rate.
    Auto = 2,
}

impl RoundingMode {
    /// Returns the unshifted rounding-mode encoding.
    pub const fn bits(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for RoundingMode {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Down),
            1 => Ok(Self::Up),
            2 => Ok(Self::Auto),
            _ => Err(value),
        }
    }
}

impl From<RoundingMode> for u8 {
    fn from(value: RoundingMode) -> Self {
        value.bits()
    }
}

/// The complete flags word accepted by [`SET_RATE`].
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SetRateFlags(u32);

impl SetRateFlags {
    const ROUNDING_SHIFT: u32 = 0;
    const ROUNDING_MASK: u32 = 0b11 << Self::ROUNDING_SHIFT;
    const RESERVED_MASK: u32 = !Self::ROUNDING_MASK;

    /// Creates a flags word with reserved bits cleared.
    pub const fn new(rounding_mode: RoundingMode) -> Self {
        Self((rounding_mode.bits() as u32) << Self::ROUNDING_SHIFT)
    }

    /// Creates a flags word from its bit representation without validation.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the flags word's bit representation.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns the rounding mode, or its unrecognized encoding.
    pub fn rounding_mode(self) -> Result<RoundingMode, u8> {
        RoundingMode::try_from(((self.0 & Self::ROUNDING_MASK) >> Self::ROUNDING_SHIFT) as u8)
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}

/// A clock rate expressed as low and high logical RPMI words.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rate {
    low: u32,
    high: u32,
}

impl Rate {
    /// Encodes a rate in hertz.
    pub const fn from_hz(hz: u64) -> Self {
        Self {
            low: hz as u32,
            high: (hz >> 32) as u32,
        }
    }

    /// Creates a rate from low and high logical words.
    pub const fn from_words(low: u32, high: u32) -> Self {
        Self { low, high }
    }

    /// Returns the rate in hertz.
    pub const fn hz(self) -> u64 {
        (self.low as u64) | ((self.high as u64) << 32)
    }

    /// Returns the low-word-first representation.
    pub const fn words(self) -> [u32; 2] {
        [self.low, self.high]
    }
}

/// A linear range of clock rates.
///
/// The fields are logical RPMI records, not transport-serialized bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct LinearRange {
    /// Minimum rate.
    pub min: Rate,
    /// Maximum rate.
    pub max: Rate,
    /// Rate step.
    pub step: Rate,
}
