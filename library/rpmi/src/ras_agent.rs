//! Reliability, Availability, and Serviceability agent service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::RAS_AGENT;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get the number of error sources.
pub const GET_NUM_ERROR_SOURCES: u8 = 0x02;
/// Get a list of error-source IDs.
pub const GET_ERROR_SOURCE_ID_LIST: u8 = 0x03;
/// Get an error-source descriptor.
pub const GET_ERROR_SOURCE_DESCRIPTOR: u8 = 0x04;

/// An error-source descriptor format.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum DescriptorFormat {
    /// ACPI Generic Hardware Error Source version 2 format.
    #[default]
    GhesV2 = 0x0,
    /// An implementation-specific descriptor format.
    ImplementationSpecific = 0xf,
}

impl DescriptorFormat {
    /// Returns the unshifted descriptor-format encoding.
    pub const fn bits(self) -> u8 {
        self as u8
    }
}

impl TryFrom<u8> for DescriptorFormat {
    type Error = u8;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x0 => Ok(Self::GhesV2),
            0xf => Ok(Self::ImplementationSpecific),
            _ => Err(value),
        }
    }
}

impl From<DescriptorFormat> for u8 {
    fn from(value: DescriptorFormat) -> Self {
        value.bits()
    }
}

/// The complete error-source descriptor flags word.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DescriptorFlags(u32);

impl DescriptorFlags {
    const FORMAT_SHIFT: u32 = 0;
    const FORMAT_MASK: u32 = 0b1111 << Self::FORMAT_SHIFT;
    const RESERVED_MASK: u32 = !Self::FORMAT_MASK;

    /// Creates a flags word with reserved bits cleared.
    pub const fn new(format: DescriptorFormat) -> Self {
        Self((format.bits() as u32) << Self::FORMAT_SHIFT)
    }

    /// Creates a flags word from its bit representation without validation.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the flags word's bit representation.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns the descriptor format, or its unrecognized encoding.
    pub fn format(self) -> Result<DescriptorFormat, u8> {
        DescriptorFormat::try_from(((self.0 & Self::FORMAT_MASK) >> Self::FORMAT_SHIFT) as u8)
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}
