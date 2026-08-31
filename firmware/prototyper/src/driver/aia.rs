//! IMSIC IPI device and per-hart interrupt-file initialization.

use core::sync::atomic::Ordering;

use riscv_aia::Iid;
use riscv_aia::register::mtopei;

use crate::cfg::NUM_HART_MAX;
use crate::driver::IpiSender;
use crate::platform::{AiaInfo, BoardInfo, board_info, mmio::Mmio};
use crate::riscv::csr::imsic;
use crate::riscv::current_hartid;
use crate::sbi::features::{Extension, hart_extension_probe};

/// FDT `compatible` strings identifying an IMSIC interrupt controller.
pub(crate) const IMSIC_COMPATIBLES: [&str; 2] = ["riscv,imsics", "riscv,imsic"];

/// Register-block span of one interrupt file (one 4 KiB IMSIC page).
pub(crate) const IMSIC_FILE_SPAN: usize = 0x1000;

/// `seteipnum_le` register offset within an interrupt file; a little-endian
/// 32-bit write to this register sets the pending bit of the given identity.
const SET_EIPNUM_LE: usize = 0x0;

/// IMSIC-backed IPI device delivering software interrupts as MSIs to each
/// hart's machine-level interrupt file.
pub(super) struct ImsicDevice {
    firmware_ipi_iid: Iid,
    hart_imsic_map: [Option<Mmio>; NUM_HART_MAX],
}

impl ImsicDevice {
    /// Acquires the interrupt-file MMIO pages for every mapped hart from the
    /// board's trusted regions.
    ///
    /// Returns `None` if any mapped page is not contained in a discovered
    /// region, so the caller can fall back to the CLINT backend.
    pub(super) fn new(
        firmware_ipi_iid: Iid,
        hart_imsic_map: [Option<usize>; NUM_HART_MAX],
        board: &BoardInfo,
    ) -> Option<Self> {
        let mut files = [None; NUM_HART_MAX];
        for (file, addr) in files.iter_mut().zip(hart_imsic_map) {
            if let Some(addr) = addr {
                *file = Some(Mmio::within(board, addr, IMSIC_FILE_SPAN)?);
            }
        }
        Some(Self {
            firmware_ipi_iid,
            hart_imsic_map: files,
        })
    }
}

impl IpiSender for ImsicDevice {
    #[inline(always)]
    fn send_ipi(&self, hart_id: usize) {
        let Some(file) = self.hart_imsic_map.get(hart_id).copied().flatten() else {
            warn!("IMSIC IPI: hart {} has no mapped interrupt file", hart_id);
            return;
        };
        core::sync::atomic::fence(Ordering::Release);
        file.write::<u32>(SET_EIPNUM_LE, self.firmware_ipi_iid.number() as u32);
    }

    #[inline(always)]
    fn clear_ipi(&self) {
        let _ = mtopei::claim();
    }
}

/// Initializes this hart's IMSIC when that backend was selected.
pub(crate) fn per_hart_init() {
    let Some(info) = board_info().aia.as_ref() else {
        return;
    };
    let hart_id = current_hartid();
    if hart_extension_probe(hart_id, Extension::Smaia) {
        imsic_init_hart(info);
    } else {
        warn!(
            "Hart {} lacks Smaia despite IMSIC backend selection",
            hart_id
        );
    }
}

/// Sets up this hart's machine interrupt file: delivery, thresholds, and
/// the firmware IPI interrupt enable, then enables machine externals.
fn imsic_init_hart(info: &AiaInfo) {
    let ipi_iid = usize::from(info.firmware_ipi_iid.number());
    imsic::initialize_machine_file(usize::from(info.num_ids), ipi_iid);
    debug!(
        "IMSIC: hart init done, MEIE enabled, firmware IPI IID={}",
        ipi_iid
    );
}
