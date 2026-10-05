//! Sets of validated hart identifiers.

use alloc::collections::{BTreeSet, btree_set};
use core::iter::Copied;

use super::HartId;

/// A set of enabled harts, separate from the SBI hart-mask wire type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HartSet {
    harts: BTreeSet<HartId>,
}

impl HartSet {
    /// Creates an empty set.
    pub const fn empty() -> Self {
        Self {
            harts: BTreeSet::new(),
        }
    }

    /// Inserts a validated hart.
    pub fn insert(&mut self, hart: HartId) {
        self.harts.insert(hart);
    }

    /// Returns whether the set contains `hart`.
    pub fn contains(&self, hart: HartId) -> bool {
        self.harts.contains(&hart)
    }

    /// Iterates over the harts in ascending hardware-ID order.
    pub fn iter(&self) -> HartSetIter<'_> {
        self.harts.iter().copied()
    }
}

/// Iterator over a [`HartSet`].
pub type HartSetIter<'a> = Copied<btree_set::Iter<'a, HartId>>;
