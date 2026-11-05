//! PLICSW register operations for its source/target matrix.

use core::mem::{align_of, size_of};
use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{Acknowledge, IpiDevice, Software},
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::Desc;

/// Number of source and target entries in each PLICSW matrix dimension.
const MATRIX_WIDTH: usize = 8;
const PENDING_OFFSET: usize = 0x1000;
const ENABLE_OFFSET: usize = 0x2000;
const ENABLE_STRIDE: usize = 0x80;
const CONTEXT_OFFSET: usize = 0x20_0000;
const CONTEXT_STRIDE: usize = 0x1000;
// Bit 0 of each source's eight-target group enables one target.
// Shifting the mask by the hart index selects that target.
const ALL_SOURCES_ENABLE_MASK: u32 = 0x0101_0101;

/// A PLICSW software-IPI controller.
pub(super) struct PlicSw {
    registers: MmioRegion,
}

impl PlicSw {
    /// Acquires and configures the PLICSW source/target matrix.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let registers = description.registers;
        let hart_id_upper_bound = HartId::upper_bound()?;
        if hart_id_upper_bound > MATRIX_WIDTH || !registers.has_aligned_bounds(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        let registers = memory.acquire(
            registers.subrange(0, CONTEXT_OFFSET + CONTEXT_STRIDE * hart_id_upper_bound)?,
        )?;
        for source in 1..=MATRIX_WIDTH * hart_id_upper_bound {
            registers.write(size_of::<u32>() * source, 1u32.to_le())?;
        }
        for hart in 0..hart_id_upper_bound {
            let enabled = (ALL_SOURCES_ENABLE_MASK << hart).to_le();
            let enable_offset = ENABLE_OFFSET + ENABLE_STRIDE * hart;
            registers.write(enable_offset, enabled)?;
            registers.write(enable_offset + size_of::<u32>(), enabled)?;
            registers.write(CONTEXT_OFFSET + CONTEXT_STRIDE * hart, 0u32)?;
        }
        runtime::instruction::fence();
        Ok(Self { registers })
    }
}

impl IpiDevice<Software> for PlicSw {
    fn send_ipi(&self, hart: HartId) -> Result<()> {
        let source = HartId::current()?.as_usize();
        let target = hart.as_usize();
        // The current hart owns the source row; the bit selects its target.
        let bit = MATRIX_WIDTH * source + target;
        runtime::instruction::fence();
        self.registers
            .write(
                PENDING_OFFSET + size_of::<u32>() * (bit / u32::BITS as usize),
                (1u32 << (bit % u32::BITS as usize)).to_le(),
            )
            .map_err(|_| Error::IoError)?;
        runtime::instruction::fence();
        Ok(())
    }
}

impl Acknowledge<Software> for PlicSw {
    fn acknowledge(&self, _context: &()) -> Result<bool> {
        let hart = HartId::current()?.as_usize();
        let offset = CONTEXT_OFFSET + CONTEXT_STRIDE * hart + size_of::<u32>();
        let source: u32 = self.registers.read(offset).map_err(|_| Error::IoError)?;
        if source != 0 {
            self.registers
                .write(offset, source)
                .map_err(|_| Error::IoError)?;
        }
        runtime::instruction::fence();
        Ok(true)
    }
}
