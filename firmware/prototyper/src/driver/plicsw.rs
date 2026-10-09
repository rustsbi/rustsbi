//! PLICSW software interrupts through an 8-by-8 source/target matrix.

use alloc::boxed::Box;
use core::mem::{align_of, size_of};

use runtime::hart::HartId;
use runtime::ipi::{InterruptSource, IpiDevice, IpiError, SoftwareInterruptDevice};
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

const MATRIX_WIDTH: usize = 8;
const PENDING_OFFSET: usize = 0x1000;
const ENABLE_OFFSET: usize = 0x2000;
const ENABLE_STRIDE: usize = 0x80;
const CONTEXT_OFFSET: usize = 0x20_0000;
const CONTEXT_STRIDE: usize = 0x1000;
// Bit 0 of each source's eight-target group, shifted for the selected target.
const ALL_SOURCES_ENABLE_MASK: u32 = 0x0101_0101;

struct PlicSw {
    registers: MmioRegion,
    // Exclusive raw hart ID bound; missing IDs still occupy hardware slots.
    hart_id_upper_bound: usize,
}

pub(crate) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> runtime::Result<Box<dyn IpiDevice>> {
    if hart_id_upper_bound == 0
        || hart_id_upper_bound > MATRIX_WIDTH
        || !registers.has_aligned_bounds(align_of::<u32>())
    {
        return Err(runtime::Error::InvalidArgs);
    }
    let registers = memory.acquire_mmio(
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
    registers.synchronize();
    Ok(Box::new(PlicSw {
        registers,
        hart_id_upper_bound,
    }))
}

impl IpiDevice for PlicSw {
    fn send(&self, hart: HartId) -> Result<(), IpiError> {
        let source = HartId::current().map_err(|_| IpiError)?.as_usize();
        let target = hart.as_usize();
        if source >= self.hart_id_upper_bound || target >= self.hart_id_upper_bound {
            return Err(IpiError);
        }
        let bit = MATRIX_WIDTH * source + target;
        self.registers.synchronize();
        self.registers
            .write(
                PENDING_OFFSET + size_of::<u32>() * (bit / u32::BITS as usize),
                (1u32 << (bit % u32::BITS as usize)).to_le(),
            )
            .map_err(|_| IpiError)?;
        self.registers.synchronize();
        Ok(())
    }

    fn interrupt_source(&self) -> InterruptSource<'_> {
        InterruptSource::Software(self)
    }
}

impl SoftwareInterruptDevice for PlicSw {
    fn clear(&self, hart: HartId) -> Result<(), IpiError> {
        let hart = hart.as_usize();
        if hart >= self.hart_id_upper_bound {
            return Err(IpiError);
        }
        let offset = CONTEXT_OFFSET + CONTEXT_STRIDE * hart + size_of::<u32>();
        let source: u32 = self.registers.read(offset).map_err(|_| IpiError)?;
        if source != 0 {
            self.registers.write(offset, source).map_err(|_| IpiError)?;
        }
        self.registers.synchronize();
        Ok(())
    }
}
