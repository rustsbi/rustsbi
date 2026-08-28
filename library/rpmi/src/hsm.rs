//! Hart State Management service group.

/// Service-group ID.
pub const SERVICE_GROUP_ID: u16 = crate::service_group::HART_STATE_MANAGEMENT;
/// Service-group version.
pub const SERVICE_GROUP_VERSION: crate::Version = crate::Version::new(1, 0);

/// Enable or query event notifications.
pub const ENABLE_NOTIFICATION: u8 = 0x01;
/// Get a hart's state.
pub const GET_HART_STATUS: u8 = 0x02;
/// Get the list of managed harts.
pub const GET_HART_LIST: u8 = 0x03;
/// Get supported suspend types.
pub const GET_SUSPEND_TYPES: u8 = 0x04;
/// Get timing information for a suspend type.
pub const GET_SUSPEND_INFO: u8 = 0x05;
/// Start a hart.
pub const HART_START: u8 = 0x06;
/// Stop a hart.
pub const HART_STOP: u8 = 0x07;
/// Suspend a hart.
pub const HART_SUSPEND: u8 = 0x08;

/// Hart-state values shared with the SBI HSM extension.
pub use sbi_spec::hsm::hart_state;

/// Hart-suspend type values shared with the SBI HSM extension.
pub use sbi_spec::hsm::suspend_type;

/// The complete suspend-information flags word returned by [`GET_SUSPEND_INFO`].
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct SuspendInfoFlags(u32);

impl SuspendInfoFlags {
    const LOCAL_TIMER_STOPS_MASK: u32 = 1 << 0;
    const RESERVED_MASK: u32 = !Self::LOCAL_TIMER_STOPS_MASK;

    /// Creates a flags word with reserved bits cleared.
    pub const fn new(local_timer_stops: bool) -> Self {
        Self(if local_timer_stops {
            Self::LOCAL_TIMER_STOPS_MASK
        } else {
            0
        })
    }

    /// Creates a flags word from its bit representation without validation.
    pub const fn from_bits(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns the flags word's bit representation.
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns whether the hart-local timer stops during suspend.
    pub const fn local_timer_stops(self) -> bool {
        self.0 & Self::LOCAL_TIMER_STOPS_MASK != 0
    }

    /// Returns bits reserved by RPMI v1.0.
    pub const fn reserved_bits(self) -> u32 {
        self.0 & Self::RESERVED_MASK
    }
}
