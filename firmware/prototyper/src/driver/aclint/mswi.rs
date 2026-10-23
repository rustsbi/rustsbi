//! ACLINT MSWI: one machine-level software-interrupt register per device hart.
//!
//! # References
//!
//! - [RISC-V ACLINT 1.0], section 3, defines the MSIP register map, the
//!   device-local HART index, and the MSIP pending semantics.
//!
//! [RISC-V ACLINT 1.0]: https://github.com/riscvarchive/riscv-aclint/blob/main/riscv-aclint.adoc

use core::mem::size_of;
use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{IpiDevice, IpiRequest},
    memory::{MemoryRegistry, MmioRegion},
};

use crate::platform::AclintMswi;

const MSIP_STRIDE: usize = size_of::<u32>();

#[repr(u32)]
enum IpiState {
    Clear = 0,
    Pending = 1,
}

pub(super) struct Mswi {
    registers: MmioRegion,
    hart_indices: crate::platform::HartIndexMap,
}

pub(super) fn bind(resource: &AclintMswi, memory: &mut MemoryRegistry) -> Result<Mswi> {
    let hart_count = resource.hart_indices.count();
    if hart_count == 0 {
        return Err(Error::InvalidArgs);
    }
    let size = hart_count.checked_mul(MSIP_STRIDE).ok_or(Error::Overflow)?;
    let registers = resource.registers.subrange(0, size)?;
    if !registers.has_aligned_bounds(MSIP_STRIDE) {
        return Err(Error::InvalidArgs);
    }
    Ok(Mswi {
        registers: memory.acquire(registers)?,
        hart_indices: resource.hart_indices.clone(),
    })
}

impl Mswi {
    fn write(&self, hart: HartId, value: IpiState) -> Result<()> {
        let index = self.hart_indices.get(hart).ok_or(Error::IoError)?;
        let offset = usize::try_from(index)
            .ok()
            .and_then(|index| index.checked_mul(MSIP_STRIDE))
            .ok_or(Error::Overflow)?;
        self.registers
            .write(offset, (value as u32).to_le())
            .map_err(|_| Error::IoError)
    }
}

impl IpiDevice for Mswi {
    fn send(&self, request: IpiRequest) -> Result<()> {
        for hart_id in request.target_hart_ids() {
            let hart = HartId::from_raw(hart_id).map_err(|_| Error::IoError)?;
            self.write(hart, IpiState::Pending)?;
        }
        Ok(())
    }

    fn acknowledge(&self) -> Result<bool> {
        self.write(HartId::current()?, IpiState::Clear)?;
        Ok(true)
    }
}
