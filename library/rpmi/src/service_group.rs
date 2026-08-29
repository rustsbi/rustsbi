//! Standard service-group identifiers and allocated ranges.

/// Base service group.
pub const BASE: u16 = 0x0001;
/// System-MSI service group.
pub const SYSTEM_MSI: u16 = 0x0002;
/// System-reset service group.
pub const SYSTEM_RESET: u16 = 0x0003;
/// System-suspend service group.
pub const SYSTEM_SUSPEND: u16 = 0x0004;
/// Hart State Management service group.
pub const HART_STATE_MANAGEMENT: u16 = 0x0005;
/// Collaborative Processor Performance Control service group.
pub const CPPC: u16 = 0x0006;
/// Voltage service group.
pub const VOLTAGE: u16 = 0x0007;
/// Clock service group.
pub const CLOCK: u16 = 0x0008;
/// Device-power service group.
pub const DEVICE_POWER: u16 = 0x0009;
/// Performance service group.
pub const PERFORMANCE: u16 = 0x000a;
/// Management-mode service group.
pub const MANAGEMENT_MODE: u16 = 0x000b;
/// Reliability, Availability, and Serviceability agent service group.
pub const RAS_AGENT: u16 = 0x000c;
/// Request-forward service group.
pub const REQUEST_FORWARD: u16 = 0x000d;
