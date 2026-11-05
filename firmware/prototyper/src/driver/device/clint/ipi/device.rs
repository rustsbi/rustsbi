//! The software-interrupt window shared by CLINT layouts.

use core::mem::{align_of, size_of};
use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{Acknowledge, IpiDevice, Software},
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::Desc;

/// Start of the `mtimecmp` array, which bounds the preceding `msip` array.
const MTIMECMP_OFFSET: usize = 0x4000;

/// A CLINT machine software-interrupt device.
pub(super) struct ClintIpi {
    registers: MmioRegion,
}

impl ClintIpi {
    /// Acquires the `msip` array through the highest enabled hart ID.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let size = HartId::upper_bound()?
            .checked_mul(size_of::<u32>())
            .ok_or(Error::Overflow)?;
        // The `msip` array must not overlap the legacy `mtimecmp` window.
        if size > MTIMECMP_OFFSET {
            return Err(Error::InvalidArgs);
        }
        let registers = description.registers.subrange(0, size)?;
        if !registers.has_aligned_bounds(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire(registers)?,
        })
    }

    fn write_msip(&self, hart: HartId, pending: bool) -> Result<()> {
        let offset = hart.as_usize() * size_of::<u32>();
        self.registers
            .write(offset, u32::from(pending))
            .map_err(|_| Error::IoError)
    }
}

impl IpiDevice<Software> for ClintIpi {
    #[inline(always)]
    fn send_ipi(&self, hart: HartId) -> Result<()> {
        self.write_msip(hart, true)
    }
}

impl Acknowledge<Software> for ClintIpi {
    #[inline(always)]
    fn acknowledge(&self, _context: &()) -> Result<bool> {
        self.write_msip(HartId::current()?, false)?;
        Ok(true)
    }
}
