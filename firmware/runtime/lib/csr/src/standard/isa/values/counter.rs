//! Counter values and access sets.
use crate::macros::csr;
use crate::value::Value;

csr! {
    /// One XLEN-wide counter word.
    pub struct CounterValue(usize);
}

impl From<usize> for CounterValue {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

/// Counter-enable bits for the three base counters and 29 performance counters.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CounterSet(u32);

/// Base counter positions in counter-enable registers.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Counter {
    Cycle = 0,
    Time = 1,
    InstructionsRetired = 2,
}

/// A programmable machine counter index, restricted to 3 through 31.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HpmCounter(u8);

impl HpmCounter {
    /// Validates an architectural programmable-counter index.
    pub const fn new(index: u16) -> Option<Self> {
        if index >= 3 && index <= 31 {
            Some(Self(index as u8))
        } else {
            None
        }
    }
    /// Returns the architectural counter index.
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// A machine counter whose accumulation firmware can control.
/// The read-only time counter is deliberately excluded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareCounter {
    Cycle,
    InstructionsRetired,
    Hpm(HpmCounter),
}

impl HardwareCounter {
    /// Validates an index into the architectural counter bank.
    pub const fn from_index(index: u16) -> Option<Self> {
        match index {
            0 => Some(Self::Cycle),
            2 => Some(Self::InstructionsRetired),
            _ => match HpmCounter::new(index) {
                Some(hpm) => Some(Self::Hpm(hpm)),
                None => None,
            },
        }
    }
    /// Returns the index shared by the machine and user counter banks.
    pub const fn index(self) -> u8 {
        match self {
            Self::Cycle => 0,
            Self::InstructionsRetired => 2,
            Self::Hpm(hpm) => hpm.index(),
        }
    }
    /// Returns the read-only S/U-mode shadow CSR number.
    pub const fn user_csr(self) -> u16 {
        0xc00 + self.index() as u16
    }
}

/// Counter-inhibit bits. Time is not an inhibit-able counter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InhibitedCounters(u32);

impl InhibitedCounters {
    /// Decodes a register snapshot, excluding the reserved time-counter bit.
    pub const fn from_bits(bits: usize) -> Self {
        Self(bits as u32 & !(1 << 1))
    }
    /// Selects all programmable counters while leaving cycle and instret running.
    pub const fn programmable() -> Self {
        Self(!0b111)
    }
    /// Selects exactly one controlled counter.
    pub const fn one(counter: HardwareCounter) -> Self {
        Self(1 << counter.index())
    }
    /// Reports whether accumulation of this counter is inhibited.
    pub const fn contains(self, counter: HardwareCounter) -> bool {
        self.0 & (1 << counter.index()) != 0
    }
}

impl Value for InhibitedCounters {
    const BITS: u32 = 32;
    fn from_bits(bits: usize) -> Self {
        Self(bits as u32 & !(1 << 1))
    }
    fn bits(self) -> usize {
        self.0 as usize
    }
}

impl CounterSet {
    /// Reports whether the selected base counter's bit is set.
    pub const fn contains(self, counter: Counter) -> bool {
        self.0 & (1 << counter as u8) != 0
    }
    /// Selects all 32 counter positions, including unimplemented counters.
    pub const fn all() -> Self {
        Self(u32::MAX)
    }
    /// Selects no counters.
    pub const fn empty() -> Self {
        Self(0)
    }
}

impl Value for CounterSet {
    const BITS: u32 = 32;
    fn from_bits(bits: usize) -> Self {
        Self(bits as u32)
    }
    fn bits(self) -> usize {
        self.0 as usize
    }
}

impl From<CounterSet> for u32 {
    fn from(value: CounterSet) -> Self {
        value.0
    }
}
