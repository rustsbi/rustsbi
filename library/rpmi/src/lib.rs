#![doc = include_str!("../README.md")]
#![no_std]
#![deny(missing_docs, unsafe_code, unstable_features)]
#![deny(rustdoc::broken_intra_doc_links)]

/// A 16-bit-major, 16-bit-minor version encoding.
///
/// RPMI uses this representation for specification, implementation, service-
/// group, and management-mode versions. This type models only their shared
/// encoding; those version fields have distinct semantics.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version(u32);

impl Version {
    /// Encodes a major and minor version.
    pub const fn new(major: u16, minor: u16) -> Self {
        Self(((major as u32) << 16) | minor as u32)
    }

    /// Creates a version from its bit representation.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the version's bit representation.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns the major version.
    pub const fn major(self) -> u16 {
        (self.0 >> 16) as u16
    }

    /// Returns the minor version.
    pub const fn minor(self) -> u16 {
        self.0 as u16
    }
}

/// Base service group definitions.
pub mod base;
/// Clock service group definitions.
pub mod clock;
/// Collaborative Processor Performance Control service group definitions.
pub mod cppc;
/// Device-power service group definitions.
pub mod device_power;
/// Hart State Management service group definitions.
pub mod hsm;
/// Management-mode service group definitions.
pub mod management_mode;
/// RPMI message protocol definitions.
pub mod message;
/// RPMI attributes used by an SBI MPXY channel.
pub mod mpxy;
/// Performance service group definitions.
pub mod performance;
/// RAS-agent service group definitions.
pub mod ras_agent;
/// Request-forward service group definitions.
pub mod request_forward;
/// Service-group identifiers and ranges.
mod service_group;
/// Shared-memory transport layout definitions.
pub mod shared_memory;
/// System-MSI service group definitions.
pub mod system_msi;
/// System-reset service group definitions.
pub mod system_reset;
/// System-suspend service group definitions.
pub mod system_suspend;
/// Voltage service group definitions.
pub mod voltage;

#[cfg(test)]
mod tests;
