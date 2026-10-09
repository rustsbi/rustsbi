//! Interrupt-device selection and next-stage interrupt policy.
//!
//! Binding fixes device selection, supervisor AIA access, FDT handoff, and
//! machine-controller protection together.

use alloc::boxed::Box;
use runtime::hart::HartId;
use runtime::ipi::IpiDevice;
use runtime::memory::{MemoryRegistry, PhysAddr, PhysAddrRange};
use runtime::timer::TimerDevice;

use super::info::{BoardInfo, ClintResource, ImsicInfo};
use super::qemu_aplic::QemuAplicConfig;
use crate::driver;
use crate::sbi::features::{self, Extension};

/// Selected interrupt devices and their fixed next-stage policy.
pub(crate) enum InterruptController {
    None,
    Clint {
        timer: Box<dyn TimerDevice>,
        ipi: Box<dyn IpiDevice>,
    },
    PlmtPlicsw {
        timer: Box<dyn TimerDevice>,
        ipi: Box<dyn IpiDevice>,
    },
    Imsic {
        protected_regions: Option<[PhysAddrRange; 3]>,
        ipi: Box<dyn IpiDevice>,
    },
}

impl InterruptController {
    pub(super) fn bind(board: &BoardInfo, memory: &mut MemoryRegistry) -> runtime::Result<Self> {
        let descriptions = &board.devices.interrupts;
        if let Some(imsic) = Self::select_imsic(board) {
            let (aplic, protected_regions) = if board.is_qemu_virt() {
                let aplic = *descriptions
                    .machine_aplic()
                    .ok_or(runtime::Error::InvalidArgs)?
                    .resource();
                // QEMU virt reserves a full 64 KiB machine CLINT window even
                // when the FDT describes only its implemented registers.
                let clint_start = descriptions
                    .clint()
                    .map(|node| node.resource().registers.start())
                    .unwrap_or(PhysAddr::new(0x0200_0000));
                let imsic_end = imsic
                    .hart_files
                    .iter()
                    .map(|range| range.end())
                    .max()
                    .ok_or(runtime::Error::InvalidArgs)?;
                let protected_regions = [
                    PhysAddrRange::from_start_len(clint_start, 0x1_0000)?,
                    PhysAddrRange::new(aplic.start(), aplic.end())?,
                    PhysAddrRange::new(imsic.layout.machine_base, imsic_end)?,
                ];
                if protected_regions
                    .windows(2)
                    .any(|ranges| ranges[0].end() > ranges[1].start())
                    || board
                        .memory
                        .firmware_ram_range
                        .is_none_or(|ram| imsic_end > ram.start())
                {
                    return Err(runtime::Error::InvalidArgs);
                }
                (
                    Some(QemuAplicConfig::new(
                        aplic,
                        imsic.layout.machine_base,
                        imsic.layout.hart_index_bits,
                    )?),
                    Some(protected_regions),
                )
            } else {
                warn!("AIA: skipping QEMU virt M-APLIC setup on '{}'", board.model);
                (None, None)
            };
            // Validate the complete policy before issuing the first MMIO
            // window. Device initialization failures do not select a fallback.
            let ipi = driver::aia::bind(imsic, memory)?;
            if let Some(aplic) = aplic {
                aplic.bind(memory)?;
            }
            return Ok(Self::Imsic {
                protected_regions,
                ipi,
            });
        }

        if let (Some(plmt), Some(plicsw)) = (descriptions.plmt, descriptions.plicsw) {
            let upper_bound = Self::hart_id_upper_bound()?;
            let timer = Box::new(driver::plmt::bind(plmt, memory, upper_bound)?);
            let ipi = driver::plicsw::bind(plicsw, memory, upper_bound)?;
            return Ok(Self::PlmtPlicsw { timer, ipi });
        }
        if let Some(clint) = descriptions.clint() {
            let ClintResource { registers, kind } = *clint.resource();
            let (timer, ipi) =
                driver::clint::bind(registers, kind, memory, Self::hart_id_upper_bound()?)?;
            return Ok(Self::Clint { timer, ipi });
        }
        Ok(Self::None)
    }

    /// Returns the selected MMIO timer, if one is bound.
    pub(super) fn timer(&self) -> Option<&dyn TimerDevice> {
        match self {
            Self::Clint { timer, .. } | Self::PlmtPlicsw { timer, .. } => Some(timer.as_ref()),
            Self::None | Self::Imsic { .. } => None,
        }
    }

    /// Returns the selected IPI device, if one is bound.
    pub(super) fn ipi(&self) -> Option<&dyn IpiDevice> {
        match self {
            Self::Clint { ipi, .. } | Self::PlmtPlicsw { ipi, .. } | Self::Imsic { ipi, .. } => {
                Some(ipi.as_ref())
            }
            Self::None => None,
        }
    }

    /// Selects IMSIC only when every enabled hart supports Smaia and Sstc.
    fn select_imsic(board: &BoardInfo) -> Option<&ImsicInfo> {
        let imsic = board.devices.interrupts.imsic()?.resource();
        for hart in HartId::all() {
            if !features::hart_has_extension(hart.as_usize(), Extension::Smaia)
                || !features::hart_has_extension(hart.as_usize(), Extension::Sstc)
            {
                warn!(
                    "AIA: hart {} requires Smaia and Sstc; falling back to CLINT",
                    hart.as_usize()
                );
                return None;
            }
        }
        Some(imsic)
    }

    fn hart_id_upper_bound() -> runtime::Result<usize> {
        HartId::all()
            .last()
            .and_then(|hart| hart.as_usize().checked_add(1))
            .ok_or(runtime::Error::InvalidArgs)
    }

    pub(crate) fn supervisor_aia(&self) -> bool {
        matches!(self, Self::Imsic { .. })
    }

    pub(super) fn hidden_node_paths<'a>(
        &self,
        board: &'a BoardInfo,
    ) -> impl Iterator<Item = &'a str> {
        let supervisor_aia = self.supervisor_aia();
        board
            .devices
            .interrupts
            .aia_handoff_paths()
            .filter(move |_| supervisor_aia)
    }

    pub(super) fn protected_regions(&self) -> &[PhysAddrRange] {
        match self {
            Self::Imsic {
                protected_regions: Some(ranges),
                ..
            } => ranges,
            _ => &[],
        }
    }

    pub(super) fn log(&self, board: &BoardInfo) {
        let descriptions = &board.devices.interrupts;
        match self {
            Self::None => warn!("{:<30}: Not Available", "Platform IPI Device"),
            Self::Clint { .. } => {
                let clint = descriptions
                    .clint()
                    .expect("BUG: bound CLINT missing")
                    .resource();
                info!(
                    "{:<30}: {} (Base Address: 0x{:x})",
                    "Platform IPI Extension",
                    clint.kind.name(),
                    clint.registers.start().as_usize()
                );
            }
            Self::PlmtPlicsw { .. } => {
                let timer = descriptions.plmt.expect("BUG: bound PLMT missing");
                let ipi = descriptions.plicsw.expect("BUG: bound PLICSW missing");
                info!(
                    "{:<30}: Sunxi PLICSW (Base Address: 0x{:x})",
                    "Platform IPI Extension",
                    ipi.start().as_usize()
                );
                info!(
                    "{:<30}: Andes PLMT (Base Address: 0x{:x})",
                    "Platform Timer Extension",
                    timer.start().as_usize()
                );
            }
            Self::Imsic { .. } => {
                let imsic = descriptions
                    .imsic()
                    .expect("BUG: bound IMSIC missing")
                    .resource();
                info!(
                    "{:<30}: IMSIC (M-level Base Address: 0x{:x})",
                    "Platform IPI Extension",
                    imsic.layout.machine_base.as_usize()
                );
            }
        }
    }
}
