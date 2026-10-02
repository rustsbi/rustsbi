//! ACLINT MSWI: per-hart MSIP at +4.

use super::{IpiBackend, IpiError, IpiRequest};
use crate::cfg::NUM_HART_MAX;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

const MSIP_STRIDE: usize = 4;

#[repr(u32)]
enum IpiState {
    Clear = 0,
    Pending = 1,
}

pub(in crate::driver) struct Mswi {
    registers: MmioRegion,
    hart_count: usize,
}

pub(in crate::driver) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_count: usize,
) -> runtime::Result<Mswi> {
    if hart_count == 0 || hart_count > NUM_HART_MAX {
        return Err(runtime::Error::InvalidArgs);
    }
    let window = registers.subrange(0, MSIP_STRIDE * hart_count)?;
    if !window.has_aligned_bounds(MSIP_STRIDE) {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(Mswi {
        registers: memory.acquire_mmio(window)?,
        hart_count,
    })
}

impl Mswi {
    fn write(&self, hart_id: usize, value: IpiState) -> Result<(), IpiError> {
        if hart_id >= self.hart_count {
            return Err(IpiError::Failed);
        }
        self.registers
            .write(MSIP_STRIDE * hart_id, value as u32)
            .map_err(|_| IpiError::Failed)
    }
}

impl IpiBackend for Mswi {
    #[inline(always)]
    fn send_ipi(&self, request: IpiRequest) -> Result<(), IpiError> {
        for hart_id in request.harts() {
            self.write(hart_id, IpiState::Pending)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn clear_ipi(&self, hart_id: usize) -> Result<(), IpiError> {
        self.write(hart_id, IpiState::Clear)
    }
}
