//! Collaborative Processor Performance Control service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::CPPC;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Probe an abstract CPPC register.
pub const PROBE_REG: u8 = 0x02;
/// Read an abstract CPPC register.
pub const READ_REG: u8 = 0x03;
/// Write an abstract CPPC register.
pub const WRITE_REG: u8 = 0x04;
/// Get the CPPC fast-channel region.
pub const GET_FAST_CHANNEL_REGION: u8 = 0x05;
/// Get one hart's CPPC fast-channel offsets.
pub const GET_FAST_CHANNEL_OFFSET: u8 = 0x06;
/// Get the list of harts supporting CPPC.
pub const GET_HART_LIST: u8 = 0x07;

/// Abstract register IDs shared with the SBI CPPC extension.
pub mod register {
    /// Highest performance.
    pub const HIGHEST_PERFORMANCE: u32 = 0x00;
    /// Nominal performance.
    pub const NOMINAL_PERFORMANCE: u32 = 0x01;
    /// Lowest nonlinear performance.
    pub const LOW_NONLINEAR_PERFORMANCE: u32 = 0x02;
    /// Lowest performance.
    pub const LOWEST_PERFORMANCE: u32 = 0x03;
    /// Guaranteed performance.
    pub const GUARANTEED_PERFORMANCE: u32 = 0x04;
    /// Desired performance.
    pub const DESIRED_PERFORMANCE: u32 = 0x05;
    /// Minimum performance.
    pub const MIN_PERFORMANCE: u32 = 0x06;
    /// Maximum performance.
    pub const MAX_PERFORMANCE: u32 = 0x07;
    /// Performance-reduction tolerance.
    pub const PERFORMANCE_REDUCTION_TOLERANCE: u32 = 0x08;
    /// Performance time window.
    pub const TIME_WINDOW: u32 = 0x09;
    /// Counter wraparound time.
    pub const COUNTER_WRAP_TIME: u32 = 0x0a;
    /// Reference performance counter.
    pub const REFERENCE_COUNTER: u32 = 0x0b;
    /// Delivered performance counter.
    pub const DELIVERED_COUNTER: u32 = 0x0c;
    /// Performance-limited indicator.
    pub const PERFORMANCE_LIMITED: u32 = 0x0d;
    /// CPPC enable control.
    pub const ENABLE: u32 = 0x0e;
    /// Autonomous selection enable control.
    pub const AUTONOMOUS_SELECTION_ENABLE: u32 = 0x0f;
    /// Autonomous activity window.
    pub const AUTONOMOUS_ACTIVITY_WINDOW: u32 = 0x10;
    /// Energy-performance preference.
    pub const ENERGY_PERFORMANCE_PREFERENCE: u32 = 0x11;
    /// Reference performance.
    pub const REFERENCE_PERFORMANCE: u32 = 0x12;
    /// Lowest frequency.
    pub const LOWEST_FREQUENCY: u32 = 0x13;
    /// Nominal frequency.
    pub const NOMINAL_FREQUENCY: u32 = 0x14;
    /// Transition latency.
    pub const TRANSITION_LATENCY: u32 = 0x8000_0000;
}

/// CPPC fast-channel layout and attribute constants.
///
/// The region size must be a power of two and its base must be 8-byte aligned.
pub mod fast_channel {
    /// Required alignment of the fast-channel region in bytes.
    pub const REGION_ALIGNMENT: usize = 8;
    /// Size of a performance-request entry in bytes.
    pub const REQUEST_SIZE: usize = 8;
    /// Size of a performance-feedback entry in bytes.
    pub const FEEDBACK_SIZE: usize = 8;
    /// Required alignment of each entry in bytes.
    pub const ENTRY_ALIGNMENT: usize = REGION_ALIGNMENT;
    /// Desired-performance offset in a normal-mode request entry.
    pub const NORMAL_DESIRED_PERFORMANCE_OFFSET: usize = 0;
    /// Offset of the reserved word in a normal-mode request entry.
    pub const NORMAL_RESERVED_OFFSET: usize = 4;
    /// Minimum-performance offset in an autonomous-mode request entry.
    pub const AUTONOMOUS_MIN_PERFORMANCE_OFFSET: usize = 0;
    /// Maximum-performance offset in an autonomous-mode request entry.
    pub const AUTONOMOUS_MAX_PERFORMANCE_OFFSET: usize = 4;
    /// Low-word offset of delivered frequency in a feedback entry.
    pub const FEEDBACK_FREQUENCY_LOW_OFFSET: usize = 0;
    /// High-word offset of delivered frequency in a feedback entry.
    pub const FEEDBACK_FREQUENCY_HIGH_OFFSET: usize = 4;

    /// A doorbell register is supported.
    pub const DOORBELL_SUPPORTED: u32 = 1 << 0;
    /// Bit position of the doorbell-width encoding.
    pub const DOORBELL_WIDTH_SHIFT: u32 = 1;
    /// Mask of the doorbell-width encoding.
    pub const DOORBELL_WIDTH_MASK: u32 = 0b11 << DOORBELL_WIDTH_SHIFT;
    /// An 8-bit doorbell register.
    pub const DOORBELL_WIDTH_8: u32 = 0b00 << DOORBELL_WIDTH_SHIFT;
    /// A 16-bit doorbell register.
    pub const DOORBELL_WIDTH_16: u32 = 0b01 << DOORBELL_WIDTH_SHIFT;
    /// A 32-bit doorbell register.
    pub const DOORBELL_WIDTH_32: u32 = 0b10 << DOORBELL_WIDTH_SHIFT;
    /// Bit position of the CPPC operating mode.
    pub const MODE_SHIFT: u32 = 3;
    /// Mask of the CPPC operating mode.
    pub const MODE_MASK: u32 = 0b11 << MODE_SHIFT;
    /// Normal CPPC mode.
    pub const MODE_NORMAL: u32 = 0b00 << MODE_SHIFT;
    /// Autonomous CPPC mode.
    pub const MODE_AUTONOMOUS: u32 = 0b01 << MODE_SHIFT;
}
