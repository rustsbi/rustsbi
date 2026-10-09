//! Inter-processor interrupts and firmware IPI delivery.
//!
//! # References
//!
//! - Specification: [RISC-V SBI IPI extension](https://docs.riscv.org/reference/sbi/v3.0/ext-ipi.html) —
//!   hart-mask handling and IPI delivery semantics.

use super::{hart_local, pmu, rfence};
use alloc::vec::Vec;
use core::sync::atomic::Ordering::{Acquire, Release};
use runtime::hart::{self, HartId};
use runtime::ipi::{IpiError, IpiHandler, IpiSender};
use runtime::rustsbi::{HartMask, Ipi, SbiRet};
use sbi_spec::pmu::firmware_event;

/// IPI type for supervisor software interrupt.
pub(crate) const IPI_TYPE_SSOFT: u8 = 1 << 0;
/// IPI type for memory fence operations.
pub(crate) const IPI_TYPE_FENCE: u8 = 1 << 1;

/// SBI IPI extension and pending-work handler for the bound machine IPI device.
pub struct SbiIpi {
    sender: IpiSender,
}

impl Ipi for SbiIpi {
    /// Validates the mask and signals eligible target harts.
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        pmu::pmu_firmware_counter_increment(firmware_event::IPI_SENT);
        let requests = match target_harts(hart_mask) {
            Ok(requests) => requests,
            Err(error) => return error,
        };

        for hart in requests.iter() {
            set_ipi_type(hart.as_usize(), IPI_TYPE_SSOFT);
        }
        // Always signal: pending work can remain after a failed send.
        for hart in requests.iter() {
            if self.sender.send(hart).is_err() {
                return SbiRet::failed();
            }
        }

        SbiRet::success(0)
    }
}

impl SbiIpi {
    /// Adapts a published machine IPI device to the SBI IPI extension.
    pub(crate) fn new(sender: IpiSender) -> Self {
        Self { sender }
    }

    /// Marks remote fence work pending and signals its target hart.
    pub(super) fn send_fence_ipi(&self, hart: HartId) -> Result<(), IpiError> {
        set_ipi_type(hart.as_usize(), IPI_TYPE_FENCE);
        self.sender.send(hart)
    }
}

impl IpiHandler for SbiIpi {
    fn deliver_current(&self) -> bool {
        let ipi_type = get_and_reset_ipi_type();
        if (ipi_type & IPI_TYPE_SSOFT) != 0 {
            pmu::pmu_firmware_counter_increment(firmware_event::IPI_RECEIVED);
        }
        if (ipi_type & IPI_TYPE_FENCE) != 0 {
            rfence::rfence_handler();
        }
        ipi_type & IPI_TYPE_SSOFT != 0
    }
}

/// Marks `event_id` pending for `hart_id`, returning the previous set.
pub fn set_ipi_type(hart_id: usize, event_id: u8) -> u8 {
    hart_local::hart_local(hart_id)
        .ipi_type
        .fetch_or(event_id, Release)
}

/// Takes and clears the current hart's pending IPI types.
pub fn get_and_reset_ipi_type() -> u8 {
    let hart_id = HartId::current()
        .expect("BUG: current hart is not in the boot topology")
        .as_usize();
    hart_local::hart_local(hart_id).ipi_type.swap(0, Acquire)
}

/// Targets selected once from the SBI mask and current HSM state.
pub(super) enum TargetHarts {
    Mask(HartMask),
    Broadcast(Vec<HartId>),
}

impl TargetHarts {
    pub(super) fn iter(&self) -> impl Iterator<Item = HartId> + '_ {
        let (mask, broadcast) = match self {
            Self::Mask(mask) => (*mask, &[][..]),
            Self::Broadcast(harts) => (HartMask::from_mask_base(0, 0), harts.as_slice()),
        };
        broadcast.iter().copied().chain(mask.into_iter().map(|raw| {
            // Selection checked every ID against the immutable boot topology.
            HartId::from_raw(raw).expect("BUG: validated IPI target disappeared")
        }))
    }
}

/// Selects eligible harts after validating every requested ID.
pub(super) fn target_harts(hart_mask: HartMask) -> Result<TargetHarts, SbiRet> {
    let available = |hart: HartId| {
        crate::platform::hart_privilege_checked(hart.as_usize()) && hart::can_receive_ipi(hart)
    };
    let (mask, base) = hart_mask.into_inner();
    if base == usize::MAX {
        return Ok(TargetHarts::Broadcast(
            HartId::all().filter(|hart| available(*hart)).collect(),
        ));
    }
    let mut active_mask = 0;
    for bit in HartMask::from_mask_base(mask, 0) {
        let raw = base.checked_add(bit).ok_or_else(SbiRet::invalid_param)?;
        let hart = HartId::from_raw(raw).map_err(|_| SbiRet::invalid_param())?;
        // Stopped harts are valid targets but need no supervisor notification.
        if available(hart) {
            active_mask |= 1 << bit;
        }
    }
    Ok(TargetHarts::Mask(HartMask::from_mask_base(
        active_mask,
        base,
    )))
}
