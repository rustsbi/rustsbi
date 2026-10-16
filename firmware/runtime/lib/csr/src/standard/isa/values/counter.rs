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
