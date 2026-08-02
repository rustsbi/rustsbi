//! Validation and iteration of SBI counter masks.

use core::ops::Range;

use sbi_spec::binary::{CounterMask, Error};

#[derive(Clone, Copy)]
pub(super) struct CounterSelection {
    mask: CounterMask,
    total: usize,
}

impl CounterSelection {
    pub(super) fn new(base: usize, mask: usize, total: usize) -> Result<Self, Error> {
        if mask == 0 || base >= total {
            return Err(Error::InvalidParam);
        }
        let highest = usize::BITS as usize - 1 - mask.leading_zeros() as usize;
        if base.checked_add(highest).is_none_or(|index| index >= total) {
            return Err(Error::InvalidParam);
        }
        Ok(Self {
            mask: CounterMask::from_mask_base(mask, base),
            total,
        })
    }

    pub(super) fn first(self) -> Option<usize> {
        self.into_iter().next()
    }

    pub(super) fn take(self, count: usize) -> impl Iterator<Item = usize> {
        self.into_iter().take(count)
    }
}

impl IntoIterator for CounterSelection {
    type Item = usize;
    type IntoIter = CounterSelectionIter;

    fn into_iter(self) -> Self::IntoIter {
        let (_, base) = self.mask.into_inner();
        let end = base.saturating_add(usize::BITS as usize).min(self.total);
        CounterSelectionIter {
            mask: self.mask,
            remaining: base..end,
        }
    }
}

pub(super) struct CounterSelectionIter {
    mask: CounterMask,
    remaining: Range<usize>,
}

impl Iterator for CounterSelectionIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        self.remaining.find(|index| self.mask.has_bit(*index))
    }
}
