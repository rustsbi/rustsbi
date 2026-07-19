//! Counter identities and per-hart capability facts.

pub(super) const CYCLE_OFFSET: u8 = 0;
pub(super) const INSTRET_OFFSET: u8 = 2;
pub(super) const FIRST_PROGRAMMABLE_OFFSET: u8 = 3;
pub(super) const LAST_COUNTER_OFFSET: u8 = 31;
pub(super) const CYCLE_EVENT: usize = 1;
pub(super) const INSTRUCTION_EVENT: usize = 2;
pub(super) const EVENT_INDEX_MASK: usize = 0x000f_ffff;

/// Opaque identity of one hardware performance counter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CounterId {
    pub(super) offset: u8,
    pub(super) csr_number: u16,
}

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
    /// The identifier is not present in the calling hart's probed set.
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

#[derive(Clone, Copy)]
pub(super) struct CounterFacts {
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

impl CounterFacts {
    pub(super) const UNINITIALIZED: Self = Self {
        accessible: 0,
        controllable: 0,
        wide_events: 0,
        initialized: false,
    };

    pub(super) fn count(self) -> usize {
        self.controllable.count_ones() as usize
    }

    pub(super) fn counter(self, index: usize) -> Option<CounterId> {
        if !self.initialized {
            return None;
        }
        let mut remaining = self.controllable;
        for _ in 0..index {
            remaining &= remaining.checked_sub(1)?;
        }
        let offset = u8::try_from(remaining.trailing_zeros()).ok()?;
        (offset <= LAST_COUNTER_OFFSET).then_some(CounterId {
            offset,
            csr_number: supervisor_csr(offset),
        })
    }

    pub(super) fn validate(self, counter: CounterId) -> Result<u8, CounterError> {
        let Some(bit) = 1u32.checked_shl(u32::from(counter.offset)) else {
            return Err(CounterError::InvalidCounter);
        };
        if !self.initialized
            || self.controllable & bit == 0
            || counter.csr_number != supervisor_csr(counter.offset)
        {
            return Err(CounterError::InvalidCounter);
        }
        Ok(counter.offset)
    }

    pub(super) fn event_is_wide(self, offset: u8) -> bool {
        self.wide_events & (1u32 << offset) != 0
    }
}

pub(super) const fn supervisor_csr(offset: u8) -> u16 {
    0x0c00 + offset as u16
}
