//! IMSIC MMIO device for sending firmware IPIs.
//!
//! # References
//!
//! - Specification: [RISC-V AIA 1.0](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html),
//!   sections 2.1.5 and 2.1.8 — IMSIC MMIO pages and interrupt-file setup.

use alloc::boxed::Box;

use runtime::hart::HartId;
use runtime::memory::{MemoryRegistry, MmioRegion};

use crate::platform::ImsicInfo;
use runtime::ipi::{ImsicInterruptFile, InterruptSource, IpiDevice, IpiError};

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
struct ImsicDevice {
    interrupt_file: ImsicInterruptFile,
    hart_files: Box<[MmioRegion]>,
}

impl IpiDevice for ImsicDevice {
    fn send(&self, hart: HartId) -> Result<(), IpiError> {
        let file = self.hart_files.get(hart.index()).ok_or(IpiError)?;
        file.write(
            Register::SetEipnumLe.offset(),
            (self.interrupt_file.ipi_identity().number() as u32).to_le(),
        )
        .map_err(|_| IpiError)
    }

    fn interrupt_source(&self) -> InterruptSource<'_> {
        InterruptSource::Imsic(self.interrupt_file)
    }
}

/// Binds an IMSIC IPI sender to the selected machine interrupt files.
pub(crate) fn bind(
    imsic: &ImsicInfo,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Box<dyn IpiDevice>> {
    // The platform cannot fall back after this binding starts claiming windows.
    // Reject an invalid interrupt-file configuration before the first claim.
    let interrupt_file = ImsicInterruptFile::new(imsic.num_ids, imsic.ipi_iid)
        .map_err(|_| runtime::Error::InvalidArgs)?;
    let hart_files: runtime::Result<alloc::vec::Vec<_>> = imsic
        .hart_files
        .iter()
        .map(|register_range| memory.acquire_mmio(*register_range))
        .collect();
    let ipi = ImsicDevice {
        interrupt_file,
        hart_files: hart_files?.into_boxed_slice(),
    };

    Ok(Box::new(ipi))
}
