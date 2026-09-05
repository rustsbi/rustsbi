//! IMSIC IPI device and per-hart interrupt-file initialization.
//!
//! # References
//!
//! - Specification: [RISC-V AIA 1.0](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html),
//!   sections 2.1.5 and 2.1.8 — IMSIC MMIO pages and interrupt-file setup.

use alloc::boxed::Box;

use riscv_aia::Iid;
use riscv_aia::register::mtopei;
use runtime::memory::{MemoryRegistry, MmioRegion};

use crate::cfg::NUM_HART_MAX;
use crate::driver::{InterruptDevices, IpiDevice, SstcTimer};
use crate::platform::qemu_aplic::QemuAplicConfig;
use crate::platform::{BoardInfo, ImsicInfo, board_info};
use crate::riscv::csr::imsic;
use crate::riscv::current_hartid;
use crate::sbi::features::{Extension, hart_has_extension};

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
    hart_files: [Option<MmioRegion>; NUM_HART_MAX],
}

impl ImsicIpi {
    pub(super) fn new(ipi_iid: Iid, hart_files: [Option<MmioRegion>; NUM_HART_MAX]) -> Self {
        Self {
            ipi_iid,
            hart_files,
        }
    }
}

impl IpiDevice for ImsicIpi {
    #[inline(always)]
    fn send_ipi(&self, hart_id: usize) {
        let Some(file) = self.hart_files.get(hart_id).and_then(Option::as_ref) else {
            warn!("IMSIC IPI: hart {} has no mapped interrupt file", hart_id);
            return;
        };
        file.write(Register::SetEipnumLe.offset(), self.ipi_iid.number() as u32)
            .expect("BUG: IMSIC SETEIPNUM register escaped its interrupt-file window");
    }

    #[inline(always)]
    fn clear_ipi(&self) {
        let _ = mtopei::claim();
    }

    #[inline(always)]
    fn is_imsic(&self) -> bool {
        true
    }
}

/// Checks AIA eligibility before any MMIO window is acquired.
pub(super) fn is_eligible(board: &BoardInfo) -> bool {
    if board
        .enabled_harts
        .iter()
        .enumerate()
        .any(|(hart_id, enabled)| *enabled && !hart_supports_aia(hart_id))
    {
        warn!("AIA: requirements not met, falling back to CLINT");
        return false;
    }
    true
}

/// Binds the selected AIA interrupt devices to their MMIO windows.
pub(super) fn bind(
    imsic: &ImsicInfo,
    aplic_config: Option<QemuAplicConfig>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<InterruptDevices> {
    // No fallback is permitted after the first MMIO window is issued. All
    // hardware capability checks above therefore precede initialization.
    let mut hart_files = core::array::from_fn(|_| None);
    for (hart_file, register_range) in hart_files.iter_mut().zip(imsic.hart_files) {
        if let Some(register_range) = register_range {
            *hart_file = Some(memory.acquire_mmio(register_range)?);
        }
    }
    let ipi = ImsicIpi::new(imsic.ipi_iid, hart_files);

    if let Some(aplic_config) = aplic_config {
        aplic_config.bind(memory)?;
    }

    Ok(InterruptDevices {
        timer: Box::new(SstcTimer),
        ipi: Box::new(ipi),
    })
}

/// Initializes this hart's IMSIC when that device was selected.
pub(crate) fn initialize_hart_imsic() {
    let Some(imsic) = board_info().imsic.as_ref() else {
        return;
    };
    let hart_id = current_hartid();
    if hart_has_extension(hart_id, Extension::Smaia) {
        initialize_machine_interrupt_file(imsic);
    } else {
        warn!(
            "Hart {} lacks Smaia despite IMSIC device selection",
            hart_id
        );
    }
}

/// Sets up this hart's machine interrupt file: delivery, thresholds, and
/// the firmware IPI interrupt enable, then enables machine externals.
fn initialize_machine_interrupt_file(imsic_info: &ImsicInfo) {
    let ipi_iid = usize::from(imsic_info.ipi_iid.number());
    imsic::initialize_machine_file(usize::from(imsic_info.num_ids), ipi_iid);
    debug!(
        "IMSIC: hart init done, MEIE enabled, firmware IPI IID={}",
        ipi_iid
    );
}

fn hart_supports_aia(hart_id: usize) -> bool {
    if !hart_has_extension(hart_id, Extension::Smaia) {
        warn!("AIA: hart {} lacks Smaia, rejecting AIA", hart_id);
        return false;
    }
    if !hart_has_extension(hart_id, Extension::Sstc) {
        warn!("AIA: hart {} lacks Sstc, rejecting AIA", hart_id);
        return false;
    }
    true
}
