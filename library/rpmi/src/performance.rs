//! Performance service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::PERFORMANCE;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get the number of performance domains.
pub const GET_NUM_DOMAINS: u8 = 0x02;
/// Get attributes of a performance domain.
pub const GET_ATTRIBUTES: u8 = 0x03;
/// Get supported performance levels.
pub const GET_SUPPORTED_LEVELS: u8 = 0x04;
/// Get the current performance level.
pub const GET_LEVEL: u8 = 0x05;
/// Set the performance level.
pub const SET_LEVEL: u8 = 0x06;
/// Get the current performance limits.
pub const GET_LIMIT: u8 = 0x07;
/// Set performance limits.
pub const SET_LIMIT: u8 = 0x08;
/// Get the performance fast-channel region.
pub const GET_FAST_CHANNEL_REGION: u8 = 0x09;
/// Get fast-channel attributes for a service and domain.
pub const GET_FAST_CHANNEL_ATTRIBUTES: u8 = 0x0a;

/// A performance event ID.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EventId {
    /// The power used by a performance domain changed.
    PowerChange = 0x01,
    /// The limits of a performance domain changed.
    LimitChange = 0x02,
    /// The level of a performance domain changed.
    LevelChange = 0x03,
}

impl EventId {
    /// Returns the event-ID encoding.
    pub const fn bits(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for EventId {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(Self::PowerChange),
            0x02 => Ok(Self::LimitChange),
            0x03 => Ok(Self::LevelChange),
            _ => Err(value),
        }
    }
}

impl From<EventId> for u8 {
    fn from(value: EventId) -> Self {
        value.bits()
    }
}

/// The complete performance-domain attribute flags word.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Attributes(u32);

impl Attributes {
    const FAST_CHANNEL_SUPPORTED_MASK: u32 = 1 << 0;
    const LEVEL_CHANGE_SUPPORTED_MASK: u32 = 1 << 1;
    const LIMIT_CHANGE_SUPPORTED_MASK: u32 = 1 << 2;
    const RESERVED_MASK: u32 = !(Self::FAST_CHANNEL_SUPPORTED_MASK
        | Self::LEVEL_CHANGE_SUPPORTED_MASK
        | Self::LIMIT_CHANGE_SUPPORTED_MASK);

    /// Creates an attribute word with reserved bits cleared.
    pub const fn new(
        fast_channel_supported: bool,
        level_change_supported: bool,
        limit_change_supported: bool,
    ) -> Self {
        Self(
            if fast_channel_supported {
                Self::FAST_CHANNEL_SUPPORTED_MASK
            } else {
                0
            } | if level_change_supported {
                Self::LEVEL_CHANGE_SUPPORTED_MASK
            } else {
                0
            } | if limit_change_supported {
                Self::LIMIT_CHANGE_SUPPORTED_MASK
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

    /// Returns whether fast-channel operation is supported.
    pub const fn fast_channel_supported(self) -> bool {
        self.0 & Self::FAST_CHANNEL_SUPPORTED_MASK != 0
    }

    /// Returns whether software may change the performance level.
    pub const fn level_change_supported(self) -> bool {
        self.0 & Self::LEVEL_CHANGE_SUPPORTED_MASK != 0
    }

    /// Returns whether software may change performance limits.
    pub const fn limit_change_supported(self) -> bool {
        self.0 & Self::LIMIT_CHANGE_SUPPORTED_MASK != 0
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}

/// One supported performance-level record.
///
/// The fields are logical RPMI words, not transport-serialized bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Level {
    /// Performance-level index.
    pub index: u32,
    /// Clock frequency in kilohertz.
    pub clock_frequency_khz: u32,
    /// Power cost in microwatts.
    pub power_cost_uw: u32,
    /// Transition latency in microseconds.
    pub transition_latency_us: u32,
}

/// A performance fast-channel doorbell-register width.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DoorbellWidth {
    /// An 8-bit doorbell register.
    #[default]
    Bits8 = 0,
    /// A 16-bit doorbell register.
    Bits16 = 1,
    /// A 32-bit doorbell register.
    Bits32 = 2,
}

impl DoorbellWidth {
    /// Returns the unshifted doorbell-width encoding.
    pub const fn bits(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for DoorbellWidth {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Bits8),
            1 => Ok(Self::Bits16),
            2 => Ok(Self::Bits32),
            _ => Err(value),
        }
    }
}

impl From<DoorbellWidth> for u8 {
    fn from(value: DoorbellWidth) -> Self {
        value.bits()
    }
}

/// The complete performance fast-channel attribute flags word.
///
/// The fast-channel region size must be a power of two, and its base must be
/// aligned to [`Self::REGION_ALIGNMENT`].
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FastChannelAttributes(u32);

impl FastChannelAttributes {
    /// Required alignment of the fast-channel region in bytes.
    pub const REGION_ALIGNMENT: usize = 8;
    /// Size of a get- or set-level logical payload in bytes.
    pub const LEVEL_SIZE: usize = 4;
    /// Size of a get- or set-limit logical payload in bytes.
    pub const LIMIT_SIZE: usize = 8;
    /// Offset of the maximum level in a limit logical payload.
    pub const MAX_LEVEL_OFFSET: usize = 0;
    /// Offset of the minimum level in a limit logical payload.
    pub const MIN_LEVEL_OFFSET: usize = 4;

    const DOORBELL_SUPPORTED_MASK: u32 = 1 << 0;
    const DOORBELL_WIDTH_SHIFT: u32 = 1;
    const DOORBELL_WIDTH_MASK: u32 = 0b11 << Self::DOORBELL_WIDTH_SHIFT;
    const RESERVED_MASK: u32 = !(Self::DOORBELL_SUPPORTED_MASK | Self::DOORBELL_WIDTH_MASK);

    /// Creates an attribute word with reserved bits cleared.
    ///
    /// The doorbell-width field is unused when `doorbell_supported` is false.
    pub const fn new(doorbell_supported: bool, doorbell_width: DoorbellWidth) -> Self {
        Self(
            if doorbell_supported {
                Self::DOORBELL_SUPPORTED_MASK
            } else {
                0
            } | ((doorbell_width.bits() as u32) << Self::DOORBELL_WIDTH_SHIFT),
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

    /// Returns whether a doorbell register is supported.
    pub const fn doorbell_supported(self) -> bool {
        self.0 & Self::DOORBELL_SUPPORTED_MASK != 0
    }

    /// Returns the doorbell-register width, or its unrecognized encoding.
    ///
    /// The returned field is unused when [`Self::doorbell_supported`] is false.
    pub fn doorbell_width(self) -> Result<DoorbellWidth, u8> {
        DoorbellWidth::try_from(
            ((self.0 & Self::DOORBELL_WIDTH_MASK) >> Self::DOORBELL_WIDTH_SHIFT) as u8,
        )
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}
