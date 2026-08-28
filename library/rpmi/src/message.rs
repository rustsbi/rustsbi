//! Definitions shared by all RPMI messages.
//!
//! [`MessageHeader`](crate::message::MessageHeader) and
//! [`EventHeader`](crate::message::EventHeader) store the specification's logical
//! 32-bit words. They do not choose a byte order: that belongs to the RPMI
//! transport. In particular, the shared-memory transport serializes its words
//! in little-endian order.

/// Size of one RPMI word in bytes.
const WORD_SIZE: usize = 4;
/// Required byte alignment of a message-data length.
const DATA_LEN_ALIGNMENT: usize = WORD_SIZE;
/// Required byte alignment of an event-data length.
const EVENT_DATA_LEN_ALIGNMENT: usize = WORD_SIZE;
/// Service ID used by every notification message.
const NOTIFICATION_SERVICE_ID: u8 = 0;

/// An RPMI message type.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MessageType {
    /// A request for which an acknowledgement is expected.
    #[default]
    NormalRequest = 0,
    /// A request for which no acknowledgement is expected.
    PostedRequest = 1,
    /// An acknowledgement message.
    Acknowledgement = 2,
    /// A notification message.
    Notification = 3,
}

impl MessageType {
    /// Returns the message-type encoding.
    pub const fn bits(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for MessageType {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::NormalRequest),
            1 => Ok(Self::PostedRequest),
            2 => Ok(Self::Acknowledgement),
            3 => Ok(Self::Notification),
            _ => Err(value),
        }
    }
}

impl From<MessageType> for u8 {
    fn from(value: MessageType) -> Self {
        value.bits()
    }
}

/// An RPMI status code.
#[repr(i32)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Status {
    /// The service completed successfully.
    #[default]
    Success = 0,
    /// The service failed due to a general error.
    Failed = -1,
    /// The requested service or feature is not supported.
    NotSupported = -2,
    /// One or more parameters are invalid.
    InvalidParam = -3,
    /// The request was denied.
    Denied = -4,
    /// One or more addresses are invalid.
    InvalidAddr = -5,
    /// The operation is already in progress or the state already changed.
    Already = -6,
    /// An extension implementation or version is invalid.
    Extension = -7,
    /// The service failed due to a hardware fault.
    HwFault = -8,
    /// A required system, device, or resource is busy.
    Busy = -9,
    /// A required state is invalid.
    InvalidState = -10,
    /// An index, offset, address, or range is invalid.
    BadRange = -11,
    /// The operation timed out.
    Timeout = -12,
    /// An input/output operation failed.
    Io = -13,
    /// Requested data is unavailable.
    NoData = -14,
}

impl Status {
    /// Returns the signed status code defined by the RPMI specification.
    pub const fn code(self) -> i32 {
        self as i32
    }

    /// Returns the two's-complement encoding stored in an RPMI word.
    pub const fn bits(self) -> u32 {
        self.code() as u32
    }

    /// Returns whether this status represents success.
    pub const fn is_success(self) -> bool {
        matches!(self, Self::Success)
    }
}

impl TryFrom<u32> for Status {
    type Error = u32;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value as i32 {
            0 => Ok(Self::Success),
            -1 => Ok(Self::Failed),
            -2 => Ok(Self::NotSupported),
            -3 => Ok(Self::InvalidParam),
            -4 => Ok(Self::Denied),
            -5 => Ok(Self::InvalidAddr),
            -6 => Ok(Self::Already),
            -7 => Ok(Self::Extension),
            -8 => Ok(Self::HwFault),
            -9 => Ok(Self::Busy),
            -10 => Ok(Self::InvalidState),
            -11 => Ok(Self::BadRange),
            -12 => Ok(Self::Timeout),
            -13 => Ok(Self::Io),
            -14 => Ok(Self::NoData),
            _ => Err(value),
        }
    }
}

impl From<Status> for u32 {
    fn from(value: Status) -> Self {
        value.bits()
    }
}

/// The two logical words of an RPMI message header.
///
/// This type provides bit packing only. A transport must serialize each word
/// with the byte order selected by that transport.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct MessageHeader {
    word0: u32,
    word1: u32,
}

impl MessageHeader {
    /// Bit position of the service-group ID in message-header word 0.
    const SERVICE_GROUP_ID_SHIFT: u32 = 0;
    /// Mask of the service-group ID in message-header word 0.
    const SERVICE_GROUP_ID_MASK: u32 = 0x0000_ffff;
    /// Bit position of the service ID in message-header word 0.
    const SERVICE_ID_SHIFT: u32 = 16;
    /// Mask of the service ID in message-header word 0.
    const SERVICE_ID_MASK: u32 = 0x00ff_0000;
    /// Bit position of the flags field in message-header word 0.
    const FLAGS_SHIFT: u32 = 24;
    /// Mask of the flags field in message-header word 0.
    const FLAGS_MASK: u32 = 0xff00_0000;
    /// Bit position of the data length in message-header word 1.
    const DATA_LEN_SHIFT: u32 = 0;
    /// Mask of the data length in message-header word 1.
    const DATA_LEN_MASK: u32 = 0x0000_ffff;
    /// Bit position of the token in message-header word 1.
    const TOKEN_SHIFT: u32 = 16;
    /// Mask of the token in message-header word 1.
    const TOKEN_MASK: u32 = 0xffff_0000;
    /// Bit position of the message type in the flags field.
    const MESSAGE_TYPE_SHIFT: u8 = 0;
    /// Mask of the message type in the flags field.
    const MESSAGE_TYPE_MASK: u8 = 0b0000_0111;
    /// Bit position reserved for an RPMI transport in the flags field.
    const TRANSPORT_FLAG_SHIFT: u8 = 3;
    /// Mask reserved for an RPMI transport in the flags field.
    const TRANSPORT_FLAG_MASK: u8 = 1 << Self::TRANSPORT_FLAG_SHIFT;
    /// Mask of bits reserved by the message protocol in the flags field.
    const RESERVED_FLAGS_MASK: u8 = !(Self::MESSAGE_TYPE_MASK | Self::TRANSPORT_FLAG_MASK);

    /// Creates a header after checking common RPMI v1.0 framing invariants.
    ///
    /// Returns `None` when the data length is not a multiple of four, protocol-
    /// reserved flag bits are set, the message-type encoding is reserved, or a
    /// service ID zero is used by a non-notification message, or a notification
    /// uses a nonzero service ID. Message-data layouts and transport-specific
    /// constraints are left to their respective definitions.
    pub const fn new(
        service_group_id: u16,
        service_id: u8,
        flags: u8,
        data_len: u16,
        token: u16,
    ) -> Option<Self> {
        let message_type = (flags & Self::MESSAGE_TYPE_MASK) >> Self::MESSAGE_TYPE_SHIFT;
        let is_notification = message_type == MessageType::Notification.bits();
        if !(data_len as usize).is_multiple_of(DATA_LEN_ALIGNMENT)
            || flags & Self::RESERVED_FLAGS_MASK != 0
            || message_type > MessageType::Notification.bits()
            || (service_id == NOTIFICATION_SERVICE_ID) != is_notification
        {
            return None;
        }
        Some(Self::from_fields(
            service_group_id,
            service_id,
            flags,
            data_len,
            token,
        ))
    }

    /// Creates a header from component fields without validation.
    ///
    /// This is useful for inspecting or reproducing malformed, reserved, or
    /// future-version headers. Prefer [`Self::new`] for RPMI v1.0 messages.
    pub const fn from_fields(
        service_group_id: u16,
        service_id: u8,
        flags: u8,
        data_len: u16,
        token: u16,
    ) -> Self {
        Self {
            word0: ((service_group_id as u32) << Self::SERVICE_GROUP_ID_SHIFT)
                | ((service_id as u32) << Self::SERVICE_ID_SHIFT)
                | ((flags as u32) << Self::FLAGS_SHIFT),
            word1: ((data_len as u32) << Self::DATA_LEN_SHIFT)
                | ((token as u32) << Self::TOKEN_SHIFT),
        }
    }

    /// Creates a header from two logical words.
    pub const fn from_words(words: [u32; 2]) -> Self {
        Self {
            word0: words[0],
            word1: words[1],
        }
    }

    /// Returns the two logical words.
    pub const fn words(self) -> [u32; 2] {
        [self.word0, self.word1]
    }

    /// Returns the service-group ID.
    pub const fn service_group_id(self) -> u16 {
        ((self.word0 & Self::SERVICE_GROUP_ID_MASK) >> Self::SERVICE_GROUP_ID_SHIFT) as u16
    }

    /// Returns the service ID.
    pub const fn service_id(self) -> u8 {
        ((self.word0 & Self::SERVICE_ID_MASK) >> Self::SERVICE_ID_SHIFT) as u8
    }

    /// Returns the complete flags field.
    pub const fn flags(self) -> u8 {
        ((self.word0 & Self::FLAGS_MASK) >> Self::FLAGS_SHIFT) as u8
    }

    /// Returns the message type, or its unrecognized encoding.
    pub fn message_type(self) -> Result<MessageType, u8> {
        MessageType::try_from((self.flags() & Self::MESSAGE_TYPE_MASK) >> Self::MESSAGE_TYPE_SHIFT)
    }

    /// Returns the message-data length in bytes.
    pub const fn data_len(self) -> u16 {
        ((self.word1 & Self::DATA_LEN_MASK) >> Self::DATA_LEN_SHIFT) as u16
    }

    /// Returns the message token.
    pub const fn token(self) -> u16 {
        ((self.word1 & Self::TOKEN_MASK) >> Self::TOKEN_SHIFT) as u16
    }
}

/// The logical word at the start of an RPMI event.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EventHeader(u32);

impl EventHeader {
    /// Bit position of an event data length in an event header.
    const EVENT_DATA_LEN_SHIFT: u32 = 0;
    /// Mask of an event data length in an event header.
    const EVENT_DATA_LEN_MASK: u32 = 0x0000_ffff;
    /// Bit position of an event ID in an event header.
    const EVENT_ID_SHIFT: u32 = 16;
    /// Mask of an event ID in an event header.
    const EVENT_ID_MASK: u32 = 0x00ff_0000;

    /// Creates an event header from an event ID and valid byte length.
    ///
    /// Returns `None` when the data length is not a multiple of four.
    pub const fn new(event_id: u8, data_len: u16) -> Option<Self> {
        if !(data_len as usize).is_multiple_of(EVENT_DATA_LEN_ALIGNMENT) {
            return None;
        }
        Some(Self::from_fields(event_id, data_len))
    }

    /// Creates an event header from fields without validating the data length.
    pub const fn from_fields(event_id: u8, data_len: u16) -> Self {
        Self(
            ((event_id as u32) << Self::EVENT_ID_SHIFT)
                | ((data_len as u32) << Self::EVENT_DATA_LEN_SHIFT),
        )
    }

    /// Creates an event header from a logical word, preserving reserved bits.
    pub const fn from_word(word: u32) -> Self {
        Self(word)
    }

    /// Returns the logical event-header word.
    pub const fn word(self) -> u32 {
        self.0
    }

    /// Returns the event ID.
    pub const fn event_id(self) -> u8 {
        ((self.0 & Self::EVENT_ID_MASK) >> Self::EVENT_ID_SHIFT) as u8
    }

    /// Returns the event-data length in bytes.
    pub const fn data_len(self) -> u16 {
        ((self.0 & Self::EVENT_DATA_LEN_MASK) >> Self::EVENT_DATA_LEN_SHIFT) as u16
    }
}

/// Request data common to every `ENABLE_NOTIFICATION` service.
///
/// The fields are logical RPMI words, not transport-serialized bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EnableNotificationRequest {
    /// Event for which notification state is changed or queried.
    pub event_id: u32,
    /// Requested notification state; see [`notification_request_state`].
    pub requested_state: u32,
}

/// Response data common to every `ENABLE_NOTIFICATION` service.
///
/// The fields are logical RPMI words, not transport-serialized bytes.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EnableNotificationResponse {
    /// RPMI completion status.
    pub status: Status,
    /// Current notification state; see [`notification_state`].
    pub current_state: u32,
}

/// Values accepted as a requested notification state.
pub mod notification_request_state {
    /// Disable notification for the selected event.
    pub const DISABLE: u32 = 0;
    /// Enable notification for the selected event.
    pub const ENABLE: u32 = 1;
    /// Query the current notification state without changing it.
    pub const GET: u32 = 2;
}

/// Values returned as the current notification state.
pub mod notification_state {
    /// Notification is disabled.
    pub const DISABLED: u32 = 0;
    /// Notification is enabled.
    pub const ENABLED: u32 = 1;
}
