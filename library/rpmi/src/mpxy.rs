//! RPMI protocol-specific attributes for an SBI MPXY channel.

/// Read-only RPMI attribute IDs.
pub mod attribute {
    /// ID of the service-group-ID attribute.
    pub const SERVICE_GROUP_ID: u32 = 0x8000_0000;
    /// ID of the service-group-version attribute.
    pub const SERVICE_GROUP_VERSION: u32 = 0x8000_0001;
    /// ID of the RPMI implementation-ID attribute.
    pub const IMPLEMENTATION_ID: u32 = 0x8000_0002;
    /// ID of the RPMI implementation-version attribute.
    pub const IMPLEMENTATION_VERSION: u32 = 0x8000_0003;
}
