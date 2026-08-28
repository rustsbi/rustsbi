//! RISC-V Platform Management Interface (RPMI) message protocol.
//!
//! Defines the RPMI message header, message types, error codes and the
//! service group identifiers, matching the RISC-V RPMI specification
//! (riscv-non-isa/riscv-rpmi, `message-protocol.adoc` and
//! `service-groups.adoc`) and the reference implementation in OpenSBI
//! `include/sbi_utils/mailbox/rpmi_msgprot.h`.
//!
//! All multi-byte fields in the message header are little-endian.

/// Size of the RPMI message header in bytes.
pub const RPMI_MSG_HDR_SIZE: usize = 8;

// Message header field offsets and sizes (rpmi_msgprot.h L29-73).
/// Offset of the service group ID field within the header.
pub const RPMI_MSG_SERVICEGROUP_ID_OFFSET: usize = 0x0;
/// Size of the service group ID field.
pub const RPMI_MSG_SERVICEGROUP_ID_SIZE: usize = 2;
/// Offset of the service ID field within the header.
pub const RPMI_MSG_SERVICE_ID_OFFSET: usize = 0x2;
/// Size of the service ID field.
pub const RPMI_MSG_SERVICE_ID_SIZE: usize = 1;
/// Offset of the flags field within the header.
pub const RPMI_MSG_FLAGS_OFFSET: usize = 0x3;
/// Size of the flags field.
pub const RPMI_MSG_FLAGS_SIZE: usize = 1;
/// Offset of the data length field within the header.
pub const RPMI_MSG_DATALEN_OFFSET: usize = 0x4;
/// Size of the data length field.
pub const RPMI_MSG_DATALEN_SIZE: usize = 2;
/// Offset of the token field within the header.
pub const RPMI_MSG_TOKEN_OFFSET: usize = 0x6;
/// Size of the token field.
pub const RPMI_MSG_TOKEN_SIZE: usize = 2;

/// Offset of the message data payload within a slot.
pub const RPMI_MSG_DATA_OFFSET: usize = RPMI_MSG_HDR_SIZE;

/// Maximum payload size in a slot of the given size.
pub const fn rpmi_msg_data_size(slot_size: usize) -> usize {
    slot_size - RPMI_MSG_HDR_SIZE
}

/// Message type bit-field position and mask within `flags`.
pub const RPMI_MSG_FLAGS_TYPE_POS: usize = 0;
pub const RPMI_MSG_FLAGS_TYPE_MASK: u8 = 0x7;
/// Doorbell bit within `flags`.
pub const RPMI_MSG_FLAGS_DOORBELL_POS: usize = 3;
pub const RPMI_MSG_FLAGS_DOORBELL_MASK: u8 = 0x1;

/// Token mask (16-bit).
pub const RPMI_MSG_TOKEN_MASK: u16 = 0xffff;

/// RPMI message types (rpmi_msgprot.h L116-125).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    /// A normal request expects a response.
    NormalRequest = 0x0,
    /// A posted request does not expect a response.
    PostedRequest = 0x1,
    /// Acknowledgement of a notification.
    Acknowledgement = 0x2,
    /// Asynchronous notification from the platform.
    Notification = 0x3,
}

/// RPMI message header (rpmi_msgprot.h `struct rpmi_message_header`).
///
/// The header is 8 bytes and all multi-byte fields are little-endian.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MessageHeader {
    /// Service group identifier (little-endian u16).
    pub servicegroup_id: u16,
    /// Service identifier.
    pub service_id: u8,
    /// Message flags: type in bits 0-2, doorbell in bit 3.
    pub flags: u8,
    /// Data length in bytes (little-endian u16).
    pub datalen: u16,
    /// Token (little-endian u16).
    pub token: u16,
}

impl MessageHeader {
    /// Construct a new message header.
    pub const fn new(
        servicegroup_id: u16,
        service_id: u8,
        msg_type: MessageType,
        datalen: u16,
        token: u16,
    ) -> Self {
        Self {
            servicegroup_id,
            service_id,
            flags: msg_type as u8,
            datalen,
            token,
        }
    }

    /// Returns the message type from the flags field.
    pub const fn message_type(&self) -> MessageType {
        match self.flags & RPMI_MSG_FLAGS_TYPE_MASK {
            0x0 => MessageType::NormalRequest,
            0x1 => MessageType::PostedRequest,
            0x2 => MessageType::Acknowledgement,
            _ => MessageType::Notification,
        }
    }
}

/// RPMI error codes (rpmi_msgprot.h `enum rpmi_error`, L126-157).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum Error {
    /// Success.
    Success = 0,
    /// General failure.
    Failed = -1,
    /// Service or feature not supported.
    NotSupported = -2,
    /// Invalid parameter.
    InvalidParam = -3,
    /// Denied due to insufficient permissions or unmet prerequisite.
    Denied = -4,
    /// Invalid address or offset.
    InvalidAddr = -5,
    /// Operation failed as it was already in progress or state changed.
    Already = -6,
    /// Error in implementation which violates the specification version.
    Extension = -7,
    /// Operation failed due to hardware issues.
    HwFault = -8,
    /// System, device or resource is busy.
    Busy = -9,
    /// System or device or resource in invalid state.
    InvalidState = -10,
    /// Index, offset or address is out of range.
    BadRange = -11,
    /// Operation timed out.
    Timeout = -12,
    /// Error in input/output or sending/receiving data.
    Io = -13,
    /// No data available.
    NoData = -14,
}

impl Error {
    /// Parse an RPMI status word (first word of a response) into an error.
    pub const fn from_status(status: i32) -> Self {
        match status {
            0 => Error::Success,
            -1 => Error::Failed,
            -2 => Error::NotSupported,
            -3 => Error::InvalidParam,
            -4 => Error::Denied,
            -5 => Error::InvalidAddr,
            -6 => Error::Already,
            -7 => Error::Extension,
            -8 => Error::HwFault,
            -9 => Error::Busy,
            -10 => Error::InvalidState,
            -11 => Error::BadRange,
            -12 => Error::Timeout,
            -13 => Error::Io,
            -14 => Error::NoData,
            _ => Error::Failed,
        }
    }
}

/// RPMI service group identifiers (rpmi_msgprot.h `enum rpmi_servicegroup_id`,
/// L210-234).
pub mod servicegroup {
    /// Base service group.
    pub const BASE: u16 = 0x0001;
    /// System MSI service group.
    pub const SYSTEM_MSI: u16 = 0x0002;
    /// System reset service group.
    pub const SYSTEM_RESET: u16 = 0x0003;
    /// System suspend service group.
    pub const SYSTEM_SUSPEND: u16 = 0x0004;
    /// Hart State Management service group.
    pub const HSM: u16 = 0x0005;
    /// CPPC service group.
    pub const CPPC: u16 = 0x0006;
    /// Voltage service group.
    pub const VOLTAGE: u16 = 0x0007;
    /// Clock service group.
    pub const CLOCK: u16 = 0x0008;
    /// Device power domain service group.
    pub const DOMAIN: u16 = 0x0009;
    /// RTC service group.
    pub const RTC: u16 = 0x000e;
    /// Power key service group.
    pub const PWRKEY: u16 = 0x000f;
    /// First vendor-specific service group identifier.
    pub const VENDOR_START: u16 = 0x8000;
}

/// RPMI Base service group service identifiers (rpmi_msgprot.h
/// `enum rpmi_base_service_id`).
pub mod base_service {
    /// Enable notification.
    pub const ENABLE_NOTIFICATION: u8 = 0x01;
    /// Get implementation version.
    pub const GET_IMPLEMENTATION_VERSION: u8 = 0x02;
    /// Get implementation ID.
    pub const GET_IMPLEMENTATION_ID: u8 = 0x03;
    /// Get specification version.
    pub const GET_SPEC_VERSION: u8 = 0x04;
    /// Get platform information.
    pub const GET_PLATFORM_INFO: u8 = 0x05;
    /// Probe service group.
    pub const PROBE_SERVICE_GROUP: u8 = 0x06;
    /// Get attributes.
    pub const GET_ATTRIBUTES: u8 = 0x07;
}

/// RPMI Clock service group service identifiers (rpmi_msgprot.h
/// `enum rpmi_clock_service_id`).
pub mod clock_service {
    /// Enable notification.
    pub const ENABLE_NOTIFICATION: u8 = 0x01;
    /// Get the number of clocks.
    pub const GET_NUM_CLOCKS: u8 = 0x02;
    /// Get clock attributes.
    pub const GET_ATTRIBUTES: u8 = 0x03;
    /// Get supported clock rates.
    pub const GET_SUPPORTED_RATES: u8 = 0x04;
    /// Set clock configuration.
    pub const SET_CONFIG: u8 = 0x05;
    /// Get clock configuration.
    pub const GET_CONFIG: u8 = 0x06;
    /// Set clock rate.
    pub const SET_RATE: u8 = 0x07;
    /// Get clock rate.
    pub const GET_RATE: u8 = 0x08;
}

/// RPMI Voltage service group service identifiers (rpmi_msgprot.h
/// `enum rpmi_voltage_service_id`).
pub mod voltage_service {
    /// Enable notification.
    pub const ENABLE_NOTIFICATION: u8 = 0x01;
    /// Get the number of voltage domains.
    pub const GET_NUM_DOMAINS: u8 = 0x02;
    /// Get voltage domain attributes.
    pub const GET_ATTRIBUTES: u8 = 0x03;
    /// Get supported voltage levels.
    pub const GET_SUPPORTED_LEVELS: u8 = 0x04;
    /// Set voltage domain configuration.
    pub const SET_CONFIG: u8 = 0x05;
    /// Get voltage domain configuration.
    pub const GET_CONFIG: u8 = 0x06;
    /// Set voltage level.
    pub const SET_LEVEL: u8 = 0x07;
    /// Get voltage level.
    pub const GET_LEVEL: u8 = 0x08;
}

/// RPMI Device Power Domain service group service identifiers
/// (rpmi_msgprot.h `enum rpmi_domain_service_id`).
pub mod domain_service {
    /// Enable notification.
    pub const ENABLE_NOTIFICATION: u8 = 0x01;
    /// Get the number of power domains.
    pub const GET_NUM_DOMAINS: u8 = 0x02;
    /// Get power domain attributes.
    pub const GET_ATTRIBUTES: u8 = 0x03;
    /// Set power domain state.
    pub const SET_STATE: u8 = 0x04;
    /// Get power domain state.
    pub const GET_STATE: u8 = 0x05;
}

/// RPMI CPPC service group service identifiers (rpmi_msgprot.h
/// `enum rpmi_cppc_service_id`).
pub mod cppc_service {
    /// Enable notification.
    pub const ENABLE_NOTIFICATION: u8 = 0x01;
    /// Probe a CPPC register.
    pub const PROBE_REG: u8 = 0x02;
    /// Read a CPPC register.
    pub const READ_REG: u8 = 0x03;
    /// Write a CPPC register.
    pub const WRITE_REG: u8 = 0x04;
    /// Get the fast channel region.
    pub const GET_FAST_CHANNEL_REGION: u8 = 0x05;
    /// Get the fast channel offset.
    pub const GET_FAST_CHANNEL_OFFSET: u8 = 0x06;
    /// Get the hart list.
    pub const GET_HART_LIST: u8 = 0x07;
}

/// CPPC service request: probe a register (rpmi_msgprot.h
/// `rpmi_cppc_probe_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CppcProbeReq {
    /// Hart identifier.
    pub hart_id: u32,
    /// CPPC register identifier.
    pub reg_id: u32,
}

/// CPPC service response: probe a register (rpmi_msgprot.h
/// `rpmi_cppc_probe_resp`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CppcProbeResp {
    /// Status code.
    pub status: i32,
    /// Register width in bits (0 when not implemented).
    pub reg_len: u32,
}

/// CPPC service request: read a register (rpmi_msgprot.h
/// `rpmi_cppc_read_reg_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CppcReadReq {
    /// Hart identifier.
    pub hart_id: u32,
    /// CPPC register identifier.
    pub reg_id: u32,
}

/// CPPC service response: read a register (rpmi_msgprot.h
/// `rpmi_cppc_read_reg_resp`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CppcReadResp {
    /// Status code.
    pub status: i32,
    /// Lower 32 bits of the register value.
    pub data_lo: u32,
    /// Upper 32 bits of the register value.
    pub data_hi: u32,
}

/// CPPC service request: write a register (rpmi_msgprot.h
/// `rpmi_cppc_write_reg_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CppcWriteReq {
    /// Hart identifier.
    pub hart_id: u32,
    /// CPPC register identifier.
    pub reg_id: u32,
    /// Lower 32 bits of the register value.
    pub data_lo: u32,
    /// Upper 32 bits of the register value.
    pub data_hi: u32,
}

/// Clock configuration flags (rpmi_msgprot.h `RPMI_CLOCK_CONFIG_ENABLE`).
pub mod clock_config {
    /// Enable the clock.
    pub const ENABLE: u32 = 1 << 0;
}

/// Clock rate format in clock attributes flags
/// (rpmi_msgprot.h `RPMI_CLOCK_FLAGS_FORMAT_*`).
pub mod clock_format {
    /// Discrete clock rates.
    pub const DISCRETE: u32 = 0;
    /// Linear clock rates.
    pub const LINEAR: u32 = 1;
}

/// Clock set-rate rounding flags (rpmi_msgprot.h
/// `RPMI_CLOCK_SET_RATE_FLAGS_*`).
pub mod clock_round {
    /// Round down.
    pub const ROUND_DOWN: u32 = 0;
    /// Round up.
    pub const ROUND_UP: u32 = 1;
    /// Platform-dependent rounding.
    pub const ROUND_PLATFORM: u32 = 2;
}

/// Clock service response: number of clocks (rpmi_msgprot.h
/// `rpmi_clock_get_num_clocks_resp`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockNumClocksResp {
    /// Status code.
    pub status: i32,
    /// Number of clocks.
    pub num_clocks: u32,
}

/// Clock service request: get attributes (rpmi_msgprot.h
/// `rpmi_clock_get_attributes_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockGetAttributesReq {
    /// Clock identifier.
    pub clock_id: u32,
}

/// Clock service response: attributes (rpmi_msgprot.h
/// `rpmi_clock_get_attributes_resp`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockGetAttributesResp {
    /// Status code.
    pub status: i32,
    /// Clock flags (rate format in bits 30-31).
    pub flags: u32,
    /// Number of supported rates.
    pub num_rates: u32,
    /// Worst-case transition latency in nanoseconds.
    pub transition_latency: u32,
    /// Clock name.
    pub name: [u8; 16],
}

/// Clock service request: get supported rates (rpmi_msgprot.h
/// `rpmi_clock_get_supported_rates_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockGetSupportedRatesReq {
    /// Clock identifier.
    pub clock_id: u32,
    /// Starting rate index.
    pub clock_rate_index: u32,
}

/// Clock service request: set configuration (rpmi_msgprot.h
/// `rpmi_clock_set_config_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockSetConfigReq {
    /// Clock identifier.
    pub clock_id: u32,
    /// Configuration flags.
    pub config: u32,
}

/// Clock service request: set rate (rpmi_msgprot.h
/// `rpmi_clock_set_rate_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ClockSetRateReq {
    /// Clock identifier.
    pub clock_id: u32,
    /// Rounding flags.
    pub flags: u32,
    /// Rate low 32 bits.
    pub clock_rate_low: u32,
    /// Rate high 32 bits.
    pub clock_rate_high: u32,
}

/// Voltage service response: number of domains (rpmi_msgprot.h
/// `rpmi_voltage_get_num_domains_resp`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoltageNumDomainsResp {
    /// Status code.
    pub status: i32,
    /// Number of voltage domains.
    pub num_domains: u32,
}

/// Voltage service request: get attributes (rpmi_msgprot.h
/// `rpmi_voltage_get_attributes_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoltageGetAttributesReq {
    /// Voltage domain identifier.
    pub domain_id: u32,
}

/// Voltage service response: attributes (rpmi_msgprot.h
/// `rpmi_voltage_get_attributes_resp`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoltageGetAttributesResp {
    /// Status code.
    pub status: i32,
    /// Voltage domain flags.
    pub flags: u32,
    /// Number of supported levels.
    pub num_levels: u32,
    /// Worst-case transition latency in nanoseconds.
    pub transition_latency: u32,
    /// Voltage domain name.
    pub name: [u8; 16],
}

/// Voltage service request: get supported levels (rpmi_msgprot.h
/// `rpmi_voltage_get_supported_levels_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoltageGetSupportedLevelsReq {
    /// Voltage domain identifier.
    pub domain_id: u32,
    /// Starting level index.
    pub level_index: u32,
}

/// Voltage service request: set configuration (rpmi_msgprot.h
/// `rpmi_voltage_set_config_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoltageSetConfigReq {
    /// Voltage domain identifier.
    pub domain_id: u32,
    /// Configuration flags.
    pub config: u32,
}

/// Voltage service request: set level (rpmi_msgprot.h
/// `rpmi_voltage_set_level_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VoltageSetLevelReq {
    /// Voltage domain identifier.
    pub domain_id: u32,
    /// Voltage level index.
    pub level_index: u32,
}

/// Device power domain service response: number of domains
/// (rpmi_msgprot.h `rpmi_domain_get_num_domains_resp`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DomainNumDomainsResp {
    /// Status code.
    pub status: i32,
    /// Number of power domains.
    pub num_domains: u32,
}

/// Device power domain service request: get attributes (rpmi_msgprot.h
/// `rpmi_domain_get_attributes_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DomainGetAttributesReq {
    /// Power domain identifier.
    pub domain_id: u32,
}

/// Device power domain service response: attributes (rpmi_msgprot.h
/// `rpmi_domain_get_attributes_resp`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DomainGetAttributesResp {
    /// Status code.
    pub status: i32,
    /// Power domain flags.
    pub flags: u32,
    /// Worst-case transition latency in nanoseconds.
    pub trans_latency: u32,
    /// Power domain name.
    pub name: [u8; 16],
}

/// Device power domain service request: set state (rpmi_msgprot.h
/// `rpmi_domain_set_state_req`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DomainSetStateReq {
    /// Power domain identifier.
    pub domain_id: u32,
    /// Power state index.
    pub state: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    #[test]
    fn test_clock_struct_layout() {
        assert_eq!(core::mem::size_of::<ClockNumClocksResp>(), 8);
        assert_eq!(offset_of!(ClockNumClocksResp, num_clocks), 4);
        assert_eq!(offset_of!(ClockGetAttributesResp, name), 16);
        assert_eq!(core::mem::size_of::<ClockGetAttributesResp>(), 32);
        assert_eq!(core::mem::size_of::<ClockSetRateReq>(), 16);
    }

    #[test]
    fn test_voltage_struct_layout() {
        assert_eq!(core::mem::size_of::<VoltageNumDomainsResp>(), 8);
        assert_eq!(offset_of!(VoltageGetAttributesResp, name), 16);
        assert_eq!(core::mem::size_of::<VoltageSetLevelReq>(), 8);
    }

    #[test]
    fn test_domain_struct_layout() {
        assert_eq!(core::mem::size_of::<DomainNumDomainsResp>(), 8);
        assert_eq!(offset_of!(DomainGetAttributesResp, name), 12);
        assert_eq!(core::mem::size_of::<DomainSetStateReq>(), 8);
    }

    #[test]
    fn test_service_id_constants() {
        assert_eq!(clock_service::GET_NUM_CLOCKS, 0x02);
        assert_eq!(clock_service::SET_RATE, 0x07);
        assert_eq!(voltage_service::SET_LEVEL, 0x07);
        assert_eq!(domain_service::SET_STATE, 0x04);
        assert_eq!(clock_config::ENABLE, 1);
        assert_eq!(clock_format::LINEAR, 1);
        assert_eq!(clock_round::ROUND_PLATFORM, 2);
    }
}

/// Compose a 32-bit RPMI message identifier from its parts
/// (rpmi_msgprot.h `MAKE_MESSAGE_ID`).
pub const fn make_message_id(servicegroup_id: u16, service_id: u8, flags: u8) -> u32 {
    ((servicegroup_id as u32) << 0) | ((service_id as u32) << 16) | ((flags as u32) << 24)
}

#[cfg(test)]
mod message_tests {
    use super::*;
    use core::mem::offset_of;

    #[test]
    fn test_header_layout() {
        assert_eq!(core::mem::size_of::<MessageHeader>(), 8);
        assert_eq!(offset_of!(MessageHeader, servicegroup_id), 0x0);
        assert_eq!(offset_of!(MessageHeader, service_id), 0x2);
        assert_eq!(offset_of!(MessageHeader, flags), 0x3);
        assert_eq!(offset_of!(MessageHeader, datalen), 0x4);
        assert_eq!(offset_of!(MessageHeader, token), 0x6);
    }

    #[test]
    fn test_header_constants() {
        assert_eq!(RPMI_MSG_HDR_SIZE, 8);
        assert_eq!(RPMI_MSG_DATA_OFFSET, 8);
        assert_eq!(rpmi_msg_data_size(64), 56);
        assert_eq!(RPMI_MSG_TOKEN_MASK, 0xffff);
    }

    #[test]
    fn test_message_type() {
        let hdr = MessageHeader::new(
            servicegroup::BASE,
            base_service::GET_SPEC_VERSION,
            MessageType::NormalRequest,
            4,
            1,
        );
        assert_eq!(hdr.message_type(), MessageType::NormalRequest);
        let hdr = MessageHeader::new(servicegroup::HSM, 0, MessageType::Notification, 0, 0);
        assert_eq!(hdr.message_type(), MessageType::Notification);
    }

    #[test]
    fn test_error_parse() {
        assert_eq!(Error::from_status(0), Error::Success);
        assert_eq!(Error::from_status(-3), Error::InvalidParam);
        assert_eq!(Error::from_status(-14), Error::NoData);
    }

    #[test]
    fn test_servicegroup_ids() {
        assert_eq!(servicegroup::BASE, 0x0001);
        assert_eq!(servicegroup::HSM, 0x0005);
        assert_eq!(servicegroup::CPPC, 0x0006);
        assert_eq!(servicegroup::CLOCK, 0x0008);
        assert_eq!(servicegroup::DOMAIN, 0x0009);
        assert_eq!(servicegroup::RTC, 0x000e);
        assert_eq!(servicegroup::PWRKEY, 0x000f);
    }
}
