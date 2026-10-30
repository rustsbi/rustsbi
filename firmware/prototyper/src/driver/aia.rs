//! IMSIC IPI device and per-hart interrupt-file initialization.
//!
//! # References
//!
//! - Specification: [RISC-V AIA 1.0](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html),
//!   sections 2.1.5 and 2.1.8 — IMSIC MMIO pages and interrupt-file setup.

use alloc::boxed::Box;
use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{Iid, IpiDelivery, IpiDevice, IpiRequest},
    memory::{MemoryRegistry, MmioRegion},
};

use crate::platform::ImsicInfo;
use crate::platform::qemu_aplic::QemuAplicConfig;
use crate::riscv::csr::imsic;

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
    hart_files: Box<[MmioRegion]>,
}

impl ImsicIpi {
    pub(super) fn new(ipi_iid: Iid, hart_files: Box<[MmioRegion]>) -> Self {
        Self {
            ipi_iid,
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
        IpiDelivery::Imsic { iid: self.ipi_iid }
    }
}

/// Binds the selected AIA interrupt devices to their MMIO windows.
pub(super) fn bind(
    imsic: &ImsicInfo,
    aplic_config: Option<QemuAplicConfig>,
    memory: &mut MemoryRegistry,
) -> Result<Box<dyn IpiDevice>> {
    // No fallback is permitted after the first MMIO window is issued. All
    // hardware capability checks above therefore precede initialization.
    let hart_files: Result<alloc::vec::Vec<_>> = imsic
        .hart_files
        .iter()
        .map(|register_range| memory.acquire(*register_range))
        .collect();
    let ipi = ImsicIpi::new(imsic.ipi_iid, hart_files?.into_boxed_slice());

    if let Some(aplic_config) = aplic_config {
        aplic_config.bind(memory)?;
    }

    Ok(Box::new(ipi))
}

/// Sets up this hart's machine interrupt file: delivery, thresholds, and the
/// firmware IPI interrupt enable.
/// Runtime enables machine externals at handoff.
pub(crate) fn initialize_hart_imsic(imsic_info: &ImsicInfo) {
    let ipi_iid = usize::from(imsic_info.ipi_iid.number());
    imsic::initialize_machine_file(usize::from(imsic_info.num_ids), ipi_iid);
    debug!("IMSIC: hart init done, firmware IPI IID={}", ipi_iid);
}
