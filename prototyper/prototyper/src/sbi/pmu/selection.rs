//! Validation and iteration of SBI counter masks.

use rustsbi::SbiRet;

#[derive(Clone, Copy)]
pub(super) struct CounterSelection {
    base: usize,
    mask: usize,
}

impl CounterSelection {
    pub(super) fn new(base: usize, mask: usize, total: usize) -> Result<Self, SbiRet> {
        if mask == 0 || base >= total {
            return Err(SbiRet::invalid_param());
        }
        let highest = usize::BITS as usize - 1 - mask.leading_zeros() as usize;
        if base.checked_add(highest).is_none_or(|index| index >= total) {
            return Err(SbiRet::invalid_param());
        }
        Ok(Self { base, mask })
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
        CounterSelectionIter(self)
    }
}

pub(super) struct CounterSelectionIter(CounterSelection);

impl Iterator for CounterSelectionIter {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        let bit = self.0.mask.trailing_zeros();
        if bit == usize::BITS {
            return None;
        }
        self.0.mask &= !(1usize << bit);
        Some(self.0.base + bit as usize)
    }
}
