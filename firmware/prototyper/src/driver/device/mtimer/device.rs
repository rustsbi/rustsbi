//! Shared machine timer MMIO operations.

use core::mem::{align_of, size_of};
use runtime::{
    Error, Result,
    hart::HartId,
    memory::{MemoryRegistry, MmioRegion},
    timer::TimerDevice,
};

use super::super::hart_slot::HartSlotMap;
use super::description::{CompareAccess, Desc};

const MTIMECMP_STRIDE: usize = size_of::<u64>();

/// A machine timer with per-hart compare registers and optional MTIME.
pub(super) struct Mtimer {
    compare: MmioRegion,
    time: Option<MmioRegion>,
    hart_slots: HartSlotMap,
    access: CompareAccess,
}

impl Mtimer {
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let base_address = description.compare.start().as_usize();
        let kind = description.kind;
        let compare_size = description
            .hart_slots
            .count()
            .checked_mul(MTIMECMP_STRIDE)
            .ok_or(Error::Overflow)?;
        let compare = description.compare.subrange(0, compare_size)?;
        let compare_alignment = match description.access {
            CompareAccess::Native => align_of::<u64>(),
            CompareAccess::TwoWords => align_of::<u32>(),
        };
        if !compare.has_aligned_bounds(compare_alignment) {
            return Err(Error::InvalidArgs);
        }
        let time = description
            .time
            .map(|time| {
                let time = time.subrange(0, size_of::<u64>())?;
                if !time.has_aligned_bounds(align_of::<u64>()) {
                    return Err(Error::InvalidArgs);
                }
                memory.acquire(time)
            })
            .transpose()?;
        let device = Self {
            compare: memory.acquire(compare)?,
            time,
            hart_slots: description.hart_slots,
            access: description.access,
        };
        info!(
            "{:<30}: {} (Base Address: 0x{:x})",
            "Platform Timer Device", kind, base_address
        );
        Ok(device)
    }

    fn write_compare_words(&self, offset: usize, value: u64) -> Result<()> {
        self.compare.write(offset, u32::MAX)?;
        self.compare
            .write(offset + size_of::<u32>(), (value >> u32::BITS) as u32)?;
        self.compare.write(offset, value as u32)?;
        Ok(())
    }

    fn read_time_counter(&self) -> Result<Option<u64>> {
        let Some(time) = &self.time else {
            return Ok(None);
        };
        #[cfg(target_pointer_width = "64")]
        return time.read::<u64>(0).map(|value| Some(u64::from_le(value)));

        #[cfg(target_pointer_width = "32")]
        loop {
            let high = u32::from_le(time.read::<u32>(size_of::<u32>())?);
            let low = u32::from_le(time.read::<u32>(0)?);
            if high == u32::from_le(time.read::<u32>(size_of::<u32>())?) {
                return Ok(Some((u64::from(high) << 32) | u64::from(low)));
            }
        }
    }
}

impl TimerDevice for Mtimer {
    fn set_compare(&self, hart: HartId, deadline: u64) -> Result<()> {
        let offset = self.hart_slots.offset(hart, MTIMECMP_STRIDE)?;
        match self.access {
            CompareAccess::Native => {
                #[cfg(target_pointer_width = "64")]
                self.compare.write(offset, deadline.to_le())?;
                #[cfg(target_pointer_width = "32")]
                self.write_compare_words(offset, deadline)?;
            }
            CompareAccess::TwoWords => self.write_compare_words(offset, deadline)?,
        }
        Ok(())
    }

    fn read_time(&self) -> Result<Option<u64>> {
        self.read_time_counter()
    }
}
