//! Counter values and access sets.
//!
//! See the machine-level specification for
//! [`mcounteren`](https://docs.riscv.org/reference/isa/priv/machine.html#mcounteren),
//! [`mcountinhibit`](https://docs.riscv.org/reference/isa/priv/machine.html#mcountinhibit),
//! and [performance counters](https://docs.riscv.org/reference/isa/priv/machine.html#_hardware_performance_monitor).
use crate::macros::csr;
use crate::value::Value;

csr! {
    /// One XLEN-wide counter word.
    pub struct CounterValue(usize);
}

impl From<usize> for CounterValue {
    #[inline]
    fn from(value: usize) -> Self {
        Self(value)
    }
}

/// Subtracts two counter words using wrapping arithmetic modulo 2^XLEN.
///
/// The operands should be readings of the same counter. The result cannot
/// distinguish elapsed intervals that differ by a multiple of 2^XLEN.
///
/// See <https://docs.riscv.org/reference/isa/unpriv/counters.html>.
impl core::ops::Sub for CounterValue {
    type Output = usize;

    #[inline]
    fn sub(self, rhs: Self) -> Self::Output {
        self.0.wrapping_sub(rhs.0)
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
    /// Creates an index for counters 3 through 31, or returns `None`.
    #[inline]
    pub const fn new(index: u16) -> Option<Self> {
        if index >= 3 && index <= 31 {
            Some(Self(index as u8))
        } else {
            None
        }
    }
    /// Returns the architectural counter index.
    #[inline]
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
    /// Returns the controlled counter for an architectural index, if any.
    #[inline]
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
    #[inline]
    pub const fn index(self) -> u8 {
        match self {
            Self::Cycle => 0,
            Self::InstructionsRetired => 2,
            Self::Hpm(hpm) => hpm.index(),
        }
    }
    /// Returns the read-only S/U-mode shadow CSR number.
    #[inline]
    pub const fn user_csr(self) -> u16 {
        0xc00 + self.index() as u16
    }
}

/// Counter-inhibit bits; the time counter cannot be inhibited.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InhibitedCounters(u32);

impl InhibitedCounters {
    /// Decodes a register snapshot, excluding the reserved time-counter bit.
    #[inline]
    pub const fn from_bits(bits: usize) -> Self {
        Self(bits as u32 & !(1 << 1))
    }
    /// Selects all programmable counters while leaving cycle and instret running.
    #[inline]
    pub const fn programmable() -> Self {
        Self(!0b111)
    }
    /// Selects exactly one controlled counter.
    #[inline]
    pub const fn one(counter: HardwareCounter) -> Self {
        Self(1 << counter.index())
    }
    /// Reports whether accumulation of this counter is inhibited.
    #[inline]
    pub const fn contains(self, counter: HardwareCounter) -> bool {
        self.0 & (1 << counter.index()) != 0
    }
}

impl Value for InhibitedCounters {
    const BITS: u32 = 32;
    #[inline]
    fn from_bits(bits: usize) -> Self {
        Self(bits as u32 & !(1 << 1))
    }
    #[inline]
    fn bits(self) -> usize {
        self.0 as usize
    }
}

impl CounterSet {
    /// Reports whether the selected base counter's bit is set.
    #[inline]
    pub const fn contains(self, counter: Counter) -> bool {
        self.0 & (1 << counter as u8) != 0
    }
    /// Selects all 32 counter positions, including unimplemented counters.
    #[inline]
    pub const fn all() -> Self {
        Self(u32::MAX)
    }
    /// Selects no counters.
    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }
}

impl Value for CounterSet {
    const BITS: u32 = 32;
    #[inline]
    fn from_bits(bits: usize) -> Self {
        Self(bits as u32)
    }
    #[inline]
    fn bits(self) -> usize {
        self.0 as usize
    }
}

impl From<CounterSet> for u32 {
    #[inline]
    fn from(value: CounterSet) -> Self {
        value.0
    }
}

#[cfg(test)]
mod tests {
    use super::CounterValue;

    #[test]
    fn counter_difference_wraps_at_xlen() {
        assert_eq!(CounterValue::from(12) - CounterValue::from(7), 5);
        assert_eq!(CounterValue::from(0) - CounterValue::from(usize::MAX), 1);
    }
}
