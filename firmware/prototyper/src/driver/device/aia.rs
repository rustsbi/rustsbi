//! IMSIC interrupt files and the firmware's message sender.
//!
//! Runtime owns claiming and the CSR side of the IMSIC; this module only maps
//! the interrupt files described by the device tree and posts messages into
//! them.
//!
//! # References
//!
//! - Specification: [RISC-V AIA 1.0](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html),
//!   sections 2.1.5 and 2.1.8 — IMSIC MMIO pages and interrupt-file setup.

use alloc::boxed::Box;
use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{External, Iid, IpiDevice},
    memory::{MemoryRegistry, MmioRegion},
};

use crate::platform::ImsicInfo;
use crate::platform::qemu_aplic::QemuAplicConfig;

/// FDT `compatible` strings identifying an IMSIC interrupt controller.
pub(crate) const IMSIC_COMPATIBLES: [&str; 1] = ["riscv,imsics"];

/// Page shift of one IMSIC interrupt file.
const IMSIC_FILE_PAGE_SHIFT: u32 = 12;
pub(crate) const IMSIC_FILE_SPAN: usize = 1usize << IMSIC_FILE_PAGE_SHIFT;

/// AIA 1.0 places `seteipnum_le` at offset zero of every interrupt file.
const SET_EIPNUM_LE: usize = 0x0;

/// Binds the selected AIA interrupt devices to their MMIO windows.
pub(in crate::driver) fn bind(
    imsic: &ImsicInfo,
    aplic_config: Option<QemuAplicConfig>,
    memory: &mut MemoryRegistry,
) -> Result<Box<dyn IpiDevice<External>>> {
    // IMSIC capability is checked during discovery; after an MMIO window is
    // acquired, a binding failure cannot fall back to another IPI device.
    let hart_files: Result<Box<[_]>> = imsic
        .hart_files
        .iter()
        .map(|register_range| memory.acquire(*register_range))
        .collect::<Result<alloc::vec::Vec<_>>>()
        .map(alloc::vec::Vec::into_boxed_slice);

    if let Some(aplic_config) = aplic_config {
        aplic_config.bind(memory)?;
    }

    Ok(Box::new(ImsicIpi {
        files: hart_files?,
        iid: imsic.ipi_iid,
    }))
}

/// A firmware IPI sender backed by IMSIC interrupt files.
struct ImsicIpi {
    files: Box<[MmioRegion]>,
    iid: Iid,
}

impl IpiDevice<External> for ImsicIpi {
    fn send_ipi(&self, hart: HartId) -> Result<()> {
        let identity = u32::from(self.iid.number());
        self.files
            .get(hart.index())
            .ok_or(Error::IoError)?
            .write(SET_EIPNUM_LE, identity)
            .map_err(|_| Error::IoError)
    }
}
