//! IMSIC IPI device and per-hart interrupt-file initialization.
//!
//! # References
//!
//! - Specification: [RISC-V AIA 1.0](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html),
//!   sections 2.1.5 and 2.1.8 — IMSIC MMIO pages and interrupt-file setup.

use alloc::boxed::Box;

use riscv_aia::Iid;
use riscv_aia::register::mtopei;
use runtime::hart::HartId;
use runtime::memory::{MemoryRegistry, MmioRegion};

use crate::cfg::NUM_HART_MAX;
use crate::driver::{IpiBackend, IpiError, IpiRequest, SstcTimer, TimerBackend};
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
    hart_files: [Option<MmioRegion>; NUM_HART_MAX],
}

/// Claims the firmware IPI identity from the current machine interrupt file.
/// Constructed only after every enabled hart has passed AIA eligibility checks.
pub(crate) struct ImsicInterrupt {
    ipi_iid: Iid,
}

impl ImsicInterrupt {
    pub(crate) fn new(ipi_iid: Iid) -> Self {
        Self { ipi_iid }
    }
}

impl runtime::irq::ExternalInterrupt for ImsicInterrupt {
    fn claim_ipi(&self) -> bool {
        mtopei::claim().iid() == Some(self.ipi_iid)
    }
}

impl ImsicIpi {
    pub(super) fn new(ipi_iid: Iid, hart_files: [Option<MmioRegion>; NUM_HART_MAX]) -> Self {
        Self {
            ipi_iid,
            hart_files,
        }
    }
}

impl IpiBackend for ImsicIpi {
    #[inline(always)]
    fn send_ipi(&self, req: IpiRequest) -> Result<(), IpiError> {
        for hart_id in req.harts() {
            let file = self
                .hart_files
                .get(hart_id)
                .and_then(Option::as_ref)
                .ok_or(IpiError::Failed)?;
            file.write(Register::SetEipnumLe.offset(), self.ipi_iid.number() as u32)
                .map_err(|_| IpiError::Failed)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn clear_ipi(&self, hart_id: usize) -> Result<(), IpiError> {
        // IMSIC clearing uses CSRs on the attached hart; only the firmware
        // IPI identity is enabled in the machine interrupt file.
        if hart_id
            != HartId::current()
                .expect("BUG: current hart exceeds Runtime capacity")
                .as_usize()
        {
            return Err(IpiError::Failed);
        }
        let _ = mtopei::claim();
        Ok(())
    }

    #[inline(always)]
    fn is_imsic(&self) -> bool {
        true
    }
}

/// Binds the selected AIA interrupt devices to their MMIO windows.
pub(super) fn bind(
    imsic: &ImsicInfo,
    aplic_config: Option<QemuAplicConfig>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<(Box<dyn TimerBackend>, Box<dyn IpiBackend + Send + Sync>)> {
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

    Ok((Box::new(SstcTimer), Box::new(ipi)))
}

/// Sets up this hart's machine interrupt file: delivery, thresholds, and
/// the firmware IPI interrupt enable, then enables machine externals.
pub(crate) fn initialize_hart_imsic(imsic_info: &ImsicInfo) {
    let ipi_iid = usize::from(imsic_info.ipi_iid.number());
    imsic::initialize_machine_file(usize::from(imsic_info.num_ids), ipi_iid);
    debug!(
        "IMSIC: hart init done, MEIE enabled, firmware IPI IID={}",
        ipi_iid
    );
}
