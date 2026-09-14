//! Fixed-capacity sets of validated hart identifiers.

use super::HartId;
use crate::cfg::NUM_HART_MAX;

const WORD_BITS: usize = usize::BITS as usize;
const HART_SET_WORDS: usize = NUM_HART_MAX.div_ceil(WORD_BITS);

/// A fixed-size set of configured harts.
///
/// This is a Runtime representation, deliberately separate from the SBI
/// `HartMask` wire type.  Whether a hart is enabled is still a Prototyper
/// policy decision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HartSet {
    bits: [usize; HART_SET_WORDS],
}

impl HartSet {
    /// Creates an empty set.
    pub const fn empty() -> Self {
        Self {
            bits: [0; HART_SET_WORDS],
        }
    }

    /// Inserts a validated hart.
    #[inline]
    pub fn insert(&mut self, hart: HartId) {
        let index = hart.0 / WORD_BITS;
        let bit = hart.0 % WORD_BITS;
        self.bits[index] |= 1 << bit;
    }

    /// Returns whether the set contains `hart`.
    #[inline]
    pub fn contains(&self, hart: HartId) -> bool {
        let index = hart.0 / WORD_BITS;
        let bit = hart.0 % WORD_BITS;
        self.bits[index] & (1 << bit) != 0
    }

    /// Iterates over the harts in ascending hardware-ID order.
    #[inline]
    pub fn iter(&self) -> HartSetIter<'_> {
        HartSetIter { set: self, next: 0 }
    }
}

/// Iterator over a [`HartSet`].
pub struct HartSetIter<'a> {
    set: &'a HartSet,
    next: usize,
}

impl Iterator for HartSetIter<'_> {
    type Item = HartId;

    fn next(&mut self) -> Option<Self::Item> {
        while self.next < NUM_HART_MAX {
            let raw = self.next;
            self.next += 1;
            let hart = HartId(raw);
            if self.set.contains(hart) {
                return Some(hart);
            }
        }
        None
    }
}
