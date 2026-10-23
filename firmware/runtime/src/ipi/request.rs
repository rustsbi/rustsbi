//! Validated IPI target windows.

use super::IpiRequest;
use crate::{Error, Result, hart::HartId, rustsbi::HartMask};

impl IpiRequest {
    /// Validates every target against Runtime's published hart topology.
    /// Returns an error for broadcast encoding, overflow, or an unknown hart.
    pub fn from_mask_base(hart_mask: usize, hart_mask_base: usize) -> Result<Self> {
        if hart_mask_base == usize::MAX {
            return Err(Error::InvalidArgs);
        }
        for bit in HartMask::from_mask_base(hart_mask, 0) {
            let raw = hart_mask_base.checked_add(bit).ok_or(Error::InvalidArgs)?;
            HartId::from_raw(raw)?;
        }
        Ok(Self {
            hart_mask,
            hart_mask_base,
        })
    }

    /// Creates a window containing one validated hart.
    pub fn for_hart(hart: HartId) -> Self {
        let raw = hart.as_usize();
        let bit = raw % usize::BITS as usize;
        Self {
            hart_mask: 1 << bit,
            hart_mask_base: raw - bit,
        }
    }

    /// Iterates the validated hardware hart IDs.
    pub fn harts(self) -> impl Iterator<Item = usize> {
        HartMask::from_mask_base(self.hart_mask, self.hart_mask_base).into_iter()
    }

    /// Merges another window with the same base, retaining validated targets.
    pub fn try_merge(&mut self, other: Self) -> bool {
        if self.hart_mask_base != other.hart_mask_base {
            return false;
        }
        self.hart_mask |= other.hart_mask;
        true
    }
}
