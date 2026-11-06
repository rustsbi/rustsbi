//! Per-hart register slots for machine software interrupts and timers.

use alloc::vec::Vec;
use runtime::{Error, Result, hart::HartId};

/// Maps Runtime harts to device-local register slots.
#[derive(Clone)]
pub(crate) enum HartSlotMap {
    /// The device uses the hardware hart ID directly as its slot.
    HartId { count: usize },
    /// The device assigns slots independently of the hardware hart ID.
    Table {
        slots: Vec<Option<u32>>,
        count: usize,
    },
}

impl HartSlotMap {
    /// Uses each hardware hart ID as its register slot.
    pub(crate) fn hart_id() -> Result<Self> {
        Ok(Self::HartId {
            count: HartId::upper_bound()?,
        })
    }

    /// Creates an empty table for a device with `count` physical slots.
    pub(crate) fn table(count: usize) -> Self {
        Self::Table {
            slots: alloc::vec![None; HartId::count()],
            count,
        }
    }

    /// Assigns one device slot to an enabled hart.
    pub(crate) fn insert(&mut self, hart: HartId, slot: u32) -> bool {
        let Self::Table { slots, .. } = self else {
            return false;
        };
        let Some(entry) = slots.get_mut(hart.index()) else {
            return false;
        };
        if entry.is_some() {
            return false;
        }
        *entry = Some(slot);
        true
    }

    /// Returns the device slot for an enabled hart.
    pub(crate) fn get(&self, hart: HartId) -> Option<u32> {
        match self {
            Self::HartId { .. } => u32::try_from(hart.as_usize()).ok(),
            Self::Table { slots, .. } => slots.get(hart.index()).copied().flatten(),
        }
    }

    /// Returns whether every Runtime-enabled hart has a slot.
    pub(crate) fn covers_all_enabled(&self) -> bool {
        HartId::all().all(|hart| self.get(hart).is_some())
    }

    /// Returns the number of physical device slots.
    pub(crate) const fn count(&self) -> usize {
        match self {
            Self::HartId { count } | Self::Table { count, .. } => *count,
        }
    }

    /// Returns the byte offset for a hart when `stride` is the slot size.
    pub(crate) fn offset(&self, hart: HartId, stride: usize) -> Result<usize> {
        let slot = self.get(hart).ok_or(Error::InvalidArgs)?;
        usize::try_from(slot)
            .ok()
            .and_then(|slot| slot.checked_mul(stride))
            .ok_or(Error::Overflow)
    }
}
