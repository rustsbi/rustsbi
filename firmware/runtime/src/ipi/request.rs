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

    /// Iterates validated hart identifiers without exposing raw ABI values.
    pub fn harts(self) -> impl Iterator<Item = HartId> {
        HartMask::from_mask_base(self.hart_mask, self.hart_mask_base)
            .into_iter()
            .map(|raw| {
                // Constructors validate every target and topology is immutable.
                HartId::from_raw(raw).expect("IPI request target escaped its validated topology")
            })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hart::{HartEntry, HartLocal, publish};
    use alloc::vec::Vec;

    #[test]
    fn hart_local_and_ipi_windows_share_sparse_topology() {
        let records = HartLocal::new();
        assert_eq!(records.init_all(|_| 0), Err(Error::NotReady));
        let upper = usize::BITS as usize + 3;
        publish(
            [2, 7, upper]
                .map(|raw_id| HartEntry {
                    raw_id,
                    stack_top: 0,
                })
                .into(),
        );

        let targets = IpiRequest::from_mask_base((1 << 2) | (1 << 7), 0).unwrap();
        records.init_all(|hart| hart.as_usize()).unwrap();
        assert_eq!(
            records.init_all(|_| usize::MAX),
            Err(Error::AlreadyInitialized)
        );
        let ids: Vec<_> = targets.harts().collect();
        assert_eq!(
            ids.iter().map(|hart| hart.as_usize()).collect::<Vec<_>>(),
            [2, 7]
        );
        assert_eq!(
            ids.iter().map(|hart| hart.index()).collect::<Vec<_>>(),
            [0, 1]
        );

        let mut merged = IpiRequest::for_hart(ids[0]);
        assert!(merged.try_merge(IpiRequest::for_hart(ids[1])));
        assert_eq!(merged.harts().collect::<Vec<_>>(), ids);
        let upper = HartId::from_raw(upper).unwrap();
        for hart in HartId::all() {
            assert_eq!(*records.get(hart).unwrap(), hart.as_usize());
        }
        assert!(!merged.try_merge(IpiRequest::for_hart(upper)));
        assert_eq!(merged.harts().collect::<Vec<_>>(), ids);
        assert_eq!(
            IpiRequest::for_hart(upper).harts().collect::<Vec<_>>(),
            [upper]
        );

        for (mask, base) in [(1 << 3, 0), (0, usize::MAX), (4, usize::MAX - 1)] {
            assert_eq!(
                IpiRequest::from_mask_base(mask, base).err(),
                Some(Error::InvalidArgs)
            );
        }
        assert_eq!(IpiRequest::from_mask_base(0, 0).unwrap().harts().count(), 0);
    }
}
