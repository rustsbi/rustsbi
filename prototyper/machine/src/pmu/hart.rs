//! Per-hart hardware-counter facts discovered from RISC-V CSRs.

pub(super) const CYCLE_OFFSET: u8 = 0;
pub(super) const INSTRET_OFFSET: u8 = 2;
pub(super) const FIRST_PROGRAMMABLE_OFFSET: u8 = 3;
pub(super) const LAST_COUNTER_OFFSET: u8 = 31;
pub(super) const CYCLE_EVENT: usize = 1;
pub(super) const INSTRUCTION_EVENT: usize = 2;
pub(super) const EVENT_INDEX_MASK: usize = 0x000f_ffff;

/// Read-only architectural facts about one counter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CounterInfo {
    pub(super) csr_number: u16,
    pub(super) width: u8,
}

impl CounterInfo {
    /// Returns the 12-bit lower-privilege CSR number used by SBI encoding.
    pub const fn csr_number(self) -> u16 {
        self.csr_number
    }

    /// Returns the implemented counter width in bits.
    pub const fn width(self) -> u8 {
        self.width
    }
}

/// Failure while operating one hardware performance counter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CounterError {
    /// The dense index is not present in the calling hart's probed set.
    InvalidCounter,
    /// The counter cannot represent the requested event.
    UnsupportedEvent,
    /// Counting was already enabled.
    AlreadyStarted,
    /// Counting was already inhibited.
    AlreadyStopped,
    /// A required architectural mechanism faulted or failed readback.
    MechanismFailure,
}

/// Hardware-counter availability discovered for one admitted hart.
///
/// This type contains no SBI assignment, event-allocation, or logical running
/// policy. Its masks only describe accessible and controllable RISC-V
/// architectural counters.
#[derive(Clone, Copy)]
pub(super) struct HartCounters {
    // Access and control are independent architectural facts. A counter may
    // be readable below M-mode even when its inhibit bit is absent or locked;
    // only the controllable subset is published through the SBI PMU service.
    pub(super) accessible: u32,
    pub(super) controllable: u32,
    // RV32 platforms may implement a counter and its low event selector
    // without implementing the optional high event-selector CSR. A set bit
    // records that the complete 64-bit selector is available for this hart.
    pub(super) wide_events: u32,
    pub(super) initialized: bool,
}

impl HartCounters {
    pub(super) const UNINITIALIZED: Self = Self {
        accessible: 0,
        controllable: 0,
        wide_events: 0,
        initialized: false,
    };

    pub(super) fn count(self) -> usize {
        self.controllable.count_ones() as usize
    }

    pub(super) fn offset(self, index: usize) -> Option<u8> {
        if !self.initialized {
            return None;
        }
        let mut remaining = self.controllable;
        for _ in 0..index {
            remaining &= remaining.checked_sub(1)?;
        }
        let offset = u8::try_from(remaining.trailing_zeros()).ok()?;
        (offset <= LAST_COUNTER_OFFSET).then_some(offset)
    }

    pub(super) fn validate(self, index: usize) -> Result<u8, CounterError> {
        let Some(offset) = self.offset(index) else {
            return Err(CounterError::InvalidCounter);
        };
        let bit = 1u32 << offset;
        if !self.initialized || self.controllable & bit == 0 {
            return Err(CounterError::InvalidCounter);
        }
        Ok(offset)
    }

    pub(super) fn event_is_wide(self, offset: u8) -> bool {
        self.wide_events & (1u32 << offset) != 0
    }
}

pub(super) const fn supervisor_csr(offset: u8) -> u16 {
    0x0c00 + offset as u16
}
