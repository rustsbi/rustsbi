//! Shared machine software-interrupt MMIO operations.

use core::mem::{align_of, size_of};
use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{Acknowledge, IpiDevice, Software},
    memory::{MemoryRegistry, MmioRegion},
};

use super::super::hart_slot::HartSlotMap;
use super::description::Desc;

const MSIP_STRIDE: usize = size_of::<u32>();

/// A machine software-interrupt device backed by per-hart MSIP registers.
pub(in crate::driver) struct Mswi {
    registers: MmioRegion,
    hart_slots: HartSlotMap,
}

impl Mswi {
    pub(in crate::driver) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let size = description
            .hart_slots
            .count()
            .checked_mul(MSIP_STRIDE)
            .ok_or(Error::Overflow)?;
        let registers = description.registers.subrange(0, size)?;
        if !registers.has_aligned_bounds(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire(registers)?,
            hart_slots: description.hart_slots,
        })
    }

    fn set_pending(&self, hart: HartId, pending: bool) -> Result<()> {
        self.registers
            .write(
                self.hart_slots.offset(hart, MSIP_STRIDE)?,
                u32::from(pending),
            )
            .map_err(|_| Error::IoError)
    }
}

impl IpiDevice<Software> for Mswi {
    fn send_ipi(&self, hart: HartId) -> Result<()> {
        self.set_pending(hart, true)
    }
}

impl Acknowledge<Software> for Mswi {
    fn acknowledge(&self, _context: &()) -> Result<bool> {
        self.set_pending(HartId::current()?, false)?;
        Ok(true)
    }
}
