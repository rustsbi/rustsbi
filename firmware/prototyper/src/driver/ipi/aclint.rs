//! CLINT IPI driver.
//!
//! Both CLINT layouts expose the same MSIP window, so this class has one
//! driver and one backend.

use alloc::boxed::Box;
use core::mem::{align_of, size_of};
use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{IpiDevice, IpiRequest},
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
};

use super::{Description, Driver, Ipi, hart_bound};
use crate::devicetree::EnabledNode;
use crate::driver::clint;

pub(super) struct MswiDriver;

pub(super) static MSWI: MswiDriver = MswiDriver;

impl Driver<Ipi> for MswiDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &clint::COMPATIBLES
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a runtime::PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        let registers = platform
            .device_register(device.node())?
            .ok_or(Error::InvalidArgs)?;
        let hart_id_upper_bound = hart_bound()?;
        Ok(Some(Description::new(
            "clint_mswi",
            registers,
            move |memory| bind(registers, memory, hart_id_upper_bound),
        )))
    }
}

fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> Result<Box<dyn IpiDevice>> {
    let msip_size_bytes = hart_id_upper_bound
        .checked_mul(size_of::<u32>())
        .ok_or(Error::Overflow)?;
    if msip_size_bytes > clint::MTIMECMP_OFFSET {
        return Err(Error::InvalidArgs);
    }
    let ipi_registers = registers.subrange(0, msip_size_bytes)?;
    if !ipi_registers.has_aligned_bounds(align_of::<u32>()) {
        return Err(Error::InvalidArgs);
    }
    Ok(Box::new(ClintIpi {
        registers: memory.acquire(ipi_registers)?,
    }))
}

#[repr(u32)]
enum IpiState {
    Clear = 0,
    Pending = 1,
}

struct ClintIpi {
    registers: MmioRegion,
}

impl ClintIpi {
    fn write_msip(&self, hart_id: usize, value: IpiState) -> Result<()> {
        let offset = hart_id
            .checked_mul(size_of::<u32>())
            .ok_or(Error::IoError)?;
        self.registers
            .write(offset, value as u32)
            .map_err(|_| Error::IoError)
    }
}

impl IpiDevice for ClintIpi {
    #[inline(always)]
    fn send(&self, req: IpiRequest) -> Result<()> {
        for hart_id in req.harts() {
            self.write_msip(hart_id, IpiState::Pending)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn acknowledge(&self) -> Result<bool> {
        let hart = HartId::current()?;
        self.write_msip(hart.as_usize(), IpiState::Clear)?;
        Ok(true)
    }
}
