//! Counter access and accumulation controls.

use crate::standard::isa::values::counter::CounterSet;
use crate::{ScalarCodec, WriteOp};

/// Programmable hardware performance counter index (`3..=31`).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HpmCounter(u8);

impl HpmCounter {
    /// Returns an `HpmCounter` if `index` is in `3..=31`.
    #[inline(always)]
    pub const fn new(index: u16) -> Option<Self> {
        if index >= 3 && index <= 31 {
            Some(Self(index as u8))
        } else {
            None
        }
    }

    /// Returns the architectural counter index (`3..=31`).
    #[inline(always)]
    pub const fn index(self) -> u8 {
        self.0
    }
}

/// Machine hardware counter whose accumulation can be controlled (`time` excluded).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareCounter {
    /// Machine cycle counter (`mcycle`, index 0).
    Cycle,
    /// Machine instructions-retired counter (`minstret`, index 2).
    InstructionsRetired,
    /// Programmable counter (`mhpmcounter3..=31`).
    Hpm(HpmCounter),
}

impl HardwareCounter {
    /// Maps an architectural counter offset (`0`, `2`, or `3..=31`) to [`HardwareCounter`].
    #[inline(always)]
    pub const fn new(index: u16) -> Option<Self> {
        match index {
            0 => Some(Self::Cycle),
            2 => Some(Self::InstructionsRetired),
            _ => match HpmCounter::new(index) {
                Some(hpm) => Some(Self::Hpm(hpm)),
                None => None,
            },
        }
    }

    /// Returns the architectural counter index (`0`, `2`, or `3..=31`).
    #[inline(always)]
    pub const fn index(self) -> u8 {
        match self {
            Self::Cycle => 0,
            Self::InstructionsRetired => 2,
            Self::Hpm(hpm) => hpm.index(),
        }
    }

    /// Returns the corresponding unprivileged read-only shadow CSR number (`cycle`, `instret`, `hpmcounter3..=31`).
    #[inline(always)]
    pub const fn user_csr(self) -> u16 {
        0xc00 + self.index() as u16
    }

    /// Returns the single-bit mask inside `mcountinhibit`.
    #[inline(always)]
    pub const fn inhibit_mask(self) -> usize {
        1usize << self.index()
    }
}

/// Machine counter-inhibit bitmask (`mcountinhibit`; bit 1 for `time` is reserved zero).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InhibitedCounters(u32);

impl InhibitedCounters {
    /// Keeps the low 32 counter bits and clears reserved bit 1 (`time`).
    #[inline(always)]
    pub const fn from_bits_truncate(bits: usize) -> Self {
        Self((bits as u32) & !(1u32 << 1))
    }

    /// Returns raw bits zero-extended to `usize`.
    #[inline(always)]
    pub const fn bits(self) -> usize {
        self.0 as usize
    }

    /// Selects the programmable counter bits 3 through 31.
    #[inline(always)]
    pub const fn programmable() -> Self {
        Self(!0b111)
    }

    /// Selects one controllable hardware counter bit.
    #[inline(always)]
    pub const fn one(counter: HardwareCounter) -> Self {
        Self(1u32 << counter.index())
    }

    /// Returns whether this mask selects `counter` for inhibition.
    #[inline(always)]
    pub const fn contains(self, counter: HardwareCounter) -> bool {
        (self.0 & (1u32 << counter.index())) != 0
    }
}

impl ScalarCodec for InhibitedCounters {
    #[inline(always)]
    fn from_raw_u64(raw: u64) -> Self {
        Self::from_bits_truncate(raw as usize)
    }

    #[inline(always)]
    fn into_raw_u64(self) -> u64 {
        self.0 as u64
    }
}

define_csr! {
    /// Machine counter-enable register (`mcounteren`).
    ///
    /// # Access requirements
    ///
    /// A clear bit denies lower-privilege reads of that counter. Setting a
    /// writable bit permits access subject to lower-privilege controls. The
    /// counter continues to accumulate independently of this mask.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#mcounteren>.
    Mcounteren { 0x306, r: CounterSet, w: CounterSet };

    /// Machine counter-inhibit register (`mcountinhibit`).
    ///
    /// # Access requirements
    ///
    /// Coordinate counter accumulation with every user (including sibling harts if
    /// `CY` is shared in the implementation).
    Mcountinhibit { 0x320, r: InhibitedCounters, w: InhibitedCounters };
}

__csr_write_spec!(Mcountinhibit, crate::Set<HardwareCounter>, value =>
    WriteOp::Set(value.0.inhibit_mask())
);

__csr_write_spec!(Mcountinhibit, crate::Set<InhibitedCounters>, value =>
    WriteOp::Set(value.0.bits())
);

__csr_write_spec!(Mcountinhibit, crate::Clear<HardwareCounter>, value =>
    WriteOp::Clear(value.0.inhibit_mask())
);

define_indexed_csr! {
    /// Machine performance event selector (`mhpmevent3..=31`).
    ///
    /// # Access requirements
    ///
    /// Coordinate event selection with the corresponding counter's users.
    Mhpmevent<INDEX> {
        index: 3..=31, base: 0x320, width: crate::Width::Xlen,
        r: usize, w: usize
    }
}

define_indexed_csr! {
    /// RV32 high event selector (`mhpmevent3h..=31h`, `Sscofpmf`).
    ///
    /// # Access requirements
    ///
    /// Coordinate updates with the corresponding counter's users.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/sscofpmf.html>.
    Mhpmeventh<INDEX> {
        index: 3..=31, base: 0x720, width: crate::Width::Rv32Only,
        r: u32, w: u32
    }
}
