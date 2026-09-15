//! Sunxi PLICSW uses an 8-by-8 source/target matrix.

use super::{IpiBackend, IpiError, IpiRequest};
use crate::cfg::NUM_HART_MAX;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

pub(in crate::driver) struct PlicSw {
    registers: MmioRegion,
    hart_count: usize,
}

pub(in crate::driver) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_count: usize,
) -> runtime::Result<PlicSw> {
    if hart_count == 0
        || hart_count > 8
        || hart_count > NUM_HART_MAX
        || !registers.has_aligned_bounds(4)
    {
        return Err(runtime::Error::InvalidArgs);
    }
    let registers = memory.acquire_mmio(registers.subrange(0, 0x200000 + 0x1000 * hart_count)?)?;
    for source in 1..=8 * hart_count {
        registers.write(4 * source, 1u32.to_le())?;
    }
    for hart in 0..hart_count {
        let enabled = (0x01010101u32 << hart).to_le();
        registers.write(0x2000 + 0x80 * hart, enabled)?;
        registers.write(0x2004 + 0x80 * hart, enabled)?;
        registers.write(0x200000 + 0x1000 * hart, 0u32)?;
    }
    riscv::asm::fence();
    Ok(PlicSw {
        registers,
        hart_count,
    })
}

impl IpiBackend for PlicSw {
    fn send_ipi(&self, request: IpiRequest) -> Result<(), IpiError> {
        riscv::asm::fence();
        for hart in request.harts() {
            if hart >= self.hart_count {
                return Err(IpiError::Failed);
            }
            let source = runtime::hart::HartId::current()
                .expect("invalid current hart")
                .as_usize();
            if source >= self.hart_count {
                return Err(IpiError::Failed);
            }
            let bit = 8 * source + hart;
            self.registers
                .write(0x1000 + 4 * (bit / 32), (1u32 << (bit % 32)).to_le())
                .map_err(|_| IpiError::Failed)?;
        }
        riscv::asm::fence();
        Ok(())
    }

    fn clear_ipi(&self, hart: usize) -> Result<(), IpiError> {
        if hart >= self.hart_count
            || hart
                != runtime::hart::HartId::current()
                    .expect("invalid current hart")
                    .as_usize()
        {
            return Err(IpiError::Failed);
        }
        let offset = 0x200004 + 0x1000 * hart;
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
