//! IMSIC IPI device and per-hart interrupt-file initialization.
//!
//! # References
//!
//! - Specification: [RISC-V AIA 1.0](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html),
//!   sections 2.1.5 and 2.1.8 — IMSIC MMIO pages and interrupt-file setup.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::num::NonZeroU16;
use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{IpiDelivery, IpiDevice, IpiRequest},
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion, PhysAddr},
};

use riscv_aia::Iid;

use super::aplic::AplicConfig;

/// Address layout of the machine-level IMSIC interrupt files.
#[derive(Clone, Copy)]
pub(crate) struct ImsicAddressLayout {
    pub(crate) machine_base: PhysAddr,
    pub(crate) hart_index_bits: u32,
    group_index_shift: u32,
    hart_index_shift: u32,
}

impl ImsicAddressLayout {
    pub(crate) const fn new(
        machine_base: PhysAddr,
        hart_index_bits: u32,
        group_index_shift: u32,
        hart_index_shift: u32,
    ) -> Self {
        Self {
            machine_base,
            hart_index_bits,
            group_index_shift,
            hart_index_shift,
        }
    }

    pub(crate) fn machine_file_address(
        &self,
        hart_index: u32,
        group_index: u32,
    ) -> Option<PhysAddr> {
        let group_offset = if group_index == 0 {
            0
        } else {
            usize::try_from(group_index)
                .ok()?
                .checked_shl(self.group_index_shift)?
        };
        let hart_offset = usize::try_from(hart_index)
            .ok()?
            .checked_shl(self.hart_index_shift)?;
        self.machine_base
            .checked_add(group_offset)?
            .checked_add(hart_offset)
    }
}

/// Machine-level IMSIC resources selected from the Platform Description.
#[derive(Clone)]
pub(crate) struct ImsicInfo {
    pub(crate) layout: ImsicAddressLayout,
    pub(crate) num_ids: u16,
    pub(crate) ipi_iid: Iid,
    pub(crate) hart_files: Vec<DeviceRegisterRange>,
}

/// FDT `compatible` strings identifying an IMSIC interrupt controller.
pub(crate) const IMSIC_COMPATIBLES: [&str; 1] = ["riscv,imsics"];

/// Page shift of one IMSIC interrupt file.
const IMSIC_FILE_PAGE_SHIFT: u32 = 12;
pub(crate) const IMSIC_FILE_SPAN: usize = 1usize << IMSIC_FILE_PAGE_SHIFT;

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    SetEipnumLe = 0x0000,
}

impl Register {
    const fn offset(self) -> usize {
        self as usize
    }
}

/// IMSIC-backed IPI device delivering software interrupts as MSIs to each
/// hart's machine-level interrupt file.
pub(super) struct ImsicIpi {
    ipi_iid: Iid,
    num_ids: u16,
    hart_files: Box<[MmioRegion]>,
}

impl ImsicIpi {
    pub(super) fn new(ipi_iid: Iid, num_ids: u16, hart_files: Box<[MmioRegion]>) -> Self {
        Self {
            ipi_iid,
            num_ids,
            hart_files,
        }
    }
}

impl IpiDevice for ImsicIpi {
    #[inline(always)]
    fn send(&self, req: IpiRequest) -> Result<()> {
        for hart_id in req.harts() {
            let hart = HartId::from_raw(hart_id).map_err(|_| Error::IoError)?;
            let file = self.hart_files.get(hart.index()).ok_or(Error::IoError)?;
            file.write(Register::SetEipnumLe.offset(), self.ipi_iid.number() as u32)
                .map_err(|_| Error::IoError)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn delivery(&self) -> IpiDelivery {
        IpiDelivery::Imsic {
            iid: NonZeroU16::new(self.ipi_iid.number()).expect("validated IMSIC IPI identity"),
            num_ids: self.num_ids,
        }
    }
}

/// Binds the selected AIA interrupt devices to their MMIO windows.
pub(crate) fn bind(
    imsic: &ImsicInfo,
    aplic_config: Option<AplicConfig>,
    memory: &mut MemoryRegistry,
) -> Result<Box<dyn IpiDevice>> {
    // No fallback is permitted after the first MMIO window is issued. All
    // hardware capability checks above therefore precede initialization.
    let hart_files: Result<alloc::vec::Vec<_>> = imsic
        .hart_files
        .iter()
        .map(|register_range| memory.acquire(*register_range))
        .collect();
    let ipi = ImsicIpi::new(imsic.ipi_iid, imsic.num_ids, hart_files?.into_boxed_slice());

    if let Some(aplic_config) = aplic_config {
        aplic_config.bind(memory)?;
    }

    Ok(Box::new(ipi))
}
