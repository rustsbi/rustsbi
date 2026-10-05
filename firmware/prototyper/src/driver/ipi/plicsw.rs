//! PLICSW uses an 8-by-8 source/target matrix.

use core::mem::{align_of, size_of};

use super::{IpiBackend, IpiError, IpiRequest};
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

const MATRIX_WIDTH: usize = 8;
const PENDING_OFFSET: usize = 0x1000;
const ENABLE_OFFSET: usize = 0x2000;
const ENABLE_STRIDE: usize = 0x80;
const CONTEXT_OFFSET: usize = 0x20_0000;
const CONTEXT_STRIDE: usize = 0x1000;
// Bit 0 of each source's eight-target group, shifted for the selected target.
const ALL_SOURCES_ENABLE_MASK: u32 = 0x0101_0101;

pub(in crate::driver) struct PlicSw {
    registers: MmioRegion,
    // Exclusive raw hart ID bound; missing IDs still occupy hardware slots.
    hart_id_upper_bound: usize,
}

pub(in crate::driver) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> runtime::Result<PlicSw> {
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
    riscv::asm::fence();
    Ok(PlicSw {
        registers,
        hart_id_upper_bound,
    })
}

impl IpiBackend for PlicSw {
    fn send_ipi(&self, request: IpiRequest) -> Result<(), IpiError> {
        riscv::asm::fence();
        for hart in request.harts() {
            if hart >= self.hart_id_upper_bound {
                return Err(IpiError::Failed);
            }
            let source = runtime::hart::HartId::current()
                .expect("invalid current hart")
                .as_usize();
            if source >= self.hart_id_upper_bound {
                return Err(IpiError::Failed);
            }
            let bit = MATRIX_WIDTH * source + hart;
            self.registers
                .write(
                    PENDING_OFFSET + size_of::<u32>() * (bit / u32::BITS as usize),
                    (1u32 << (bit % u32::BITS as usize)).to_le(),
                )
                .map_err(|_| IpiError::Failed)?;
        }
        riscv::asm::fence();
        Ok(())
    }

    fn clear_ipi(&self, hart: usize) -> Result<(), IpiError> {
        if hart >= self.hart_id_upper_bound
            || hart
                != runtime::hart::HartId::current()
                    .expect("invalid current hart")
                    .as_usize()
        {
            return Err(IpiError::Failed);
        }
        let offset = CONTEXT_OFFSET + CONTEXT_STRIDE * hart + size_of::<u32>();
        let source: u32 = self.registers.read(offset).map_err(|_| IpiError::Failed)?;
        if source != 0 {
            self.registers
                .write(offset, source)
                .map_err(|_| IpiError::Failed)?;
        }
        riscv::asm::fence();
        Ok(())
    }
}
