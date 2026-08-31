//! Machine timer and IPI drivers, plus CLINT/IMSIC backend selection.

mod sifive;
mod thead;
mod kind;

use alloc::boxed::Box;

use crate::driver::{InterruptBackend, InterruptDevices, IpiSender, SstcTimer, TimerDevice, aia};
use crate::platform::mmio::Mmio;
use crate::platform::{AiaInfo, BoardInfo, cpu_enabled, qemu_aplic};
use crate::sbi::features::{Extension, hart_extension_probe};

pub(crate) use kind::ClintKind;

/// Selects an interrupt backend, preferring a fully usable IMSIC backend.
pub(super) fn from_board(board: &BoardInfo) -> Option<InterruptDevices> {
    if let Some(ref aia_info) = board.aia
        && let Some(devices) = imsic_backend(board, aia_info)
    {
        return Some(devices);
    }
    clint_backend(board)
}

fn imsic_backend(board: &BoardInfo, aia_info: &AiaInfo) -> Option<InterruptDevices> {
    let Some(enabled_harts) = cpu_enabled() else {
        warn!("AIA: enabled-hart data unavailable, falling back to CLINT");
        return None;
    };
    if enabled_harts
        .iter()
        .enumerate()
        .any(|(hart_id, enabled)| *enabled && !aia_hart_usable(hart_id))
    {
        warn!("AIA: requirements not met, falling back to CLINT");
        return None;
    }

    let ipi = aia::ImsicDevice::new(aia_info.firmware_ipi_iid, aia_info.hart_imsic_map, board)
        .or_else(|| {
            warn!("AIA: IMSIC MMIO regions unavailable, falling back to CLINT");
            None
        })?;

    if board.is_qemu_virt()
        && !qemu_aplic::init_qemu_m_aplic_delegation(
            board,
            aia_info.layout.machine_base,
            aia_info.layout.hart_index_bits,
        )
    {
        warn!("AIA: APLIC setup failed, falling back to CLINT");
        return None;
    }
    if !board.is_qemu_virt() {
        warn!("AIA: skipping QEMU virt M-APLIC setup on '{}'", board.model);
    }

    info!("AIA: IMSIC IPI + Sstc timer backend initialized");
    Some(InterruptDevices {
        backend: InterruptBackend::Imsic,
        timer: Box::new(SstcTimer),
        ipi: Box::new(ipi),
    })
}

fn clint_backend(board: &BoardInfo) -> Option<InterruptDevices> {
    let (base, kind) = board.ipi.as_ref()?;
    let span = match *kind {
        ClintKind::Sifive => sifive::SPAN,
        ClintKind::THead => thead::SPAN,
    };
    let Some(mmio) = Mmio::within(board, *base, span) else {
        warn!(
            "CLINT: FDT reg window at {:#x} is smaller than the required {:#x}-byte span, skipping",
            base, span
        );
        return None;
    };
    let (timer, ipi): (Box<dyn TimerDevice>, Box<dyn IpiSender>) = match *kind {
        ClintKind::Sifive => (
            Box::new(sifive::SifiveClint::new(mmio)),
            Box::new(sifive::SifiveClint::new(mmio)),
        ),
        ClintKind::THead => (
            Box::new(thead::THeadClint::new(mmio)),
            Box::new(thead::THeadClint::new(mmio)),
        ),
    };
    Some(InterruptDevices {
        backend: InterruptBackend::Clint,
        timer,
        ipi,
    })
}

fn aia_hart_usable(hart_id: usize) -> bool {
    if !hart_extension_probe(hart_id, Extension::Smaia) {
        warn!("AIA: hart {} lacks Smaia, rejecting AIA", hart_id);
        return false;
    }
    if !hart_extension_probe(hart_id, Extension::Sstc) {
        warn!("AIA: hart {} lacks Sstc, rejecting AIA", hart_id);
        return false;
    }
    true
}
