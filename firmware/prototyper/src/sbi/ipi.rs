//! Inter-processor interrupts and firmware IPI delivery.
//!
//! # References
//!
//! - Specification: [RISC-V SBI IPI extension](https://docs.riscv.org/reference/sbi/v3.0/ext-ipi.html) —
//!   hart-mask handling and IPI delivery semantics.

#![forbid(unsafe_code)]

use super::features::hart_privilege_checked;
use super::pmu::pmu_firmware_counter_increment;
use runtime::{
    Result,
    hart::{self, HartId},
    ipi::{self, IpiEvent},
    rustsbi::{HartMask, Ipi, SbiRet},
};

use sbi_spec::pmu::firmware_event;

/// SBI IPI extension.
///
/// The platform publishes this extension only after Runtime initializes IPI.
pub struct SbiIpi;

impl Ipi for SbiIpi {
    /// Sends IPIs to the specified harts.
    #[inline]
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        pmu_firmware_counter_increment(firmware_event::IPI_SENT);
        let targets = match target_harts(hart_mask) {
            Ok(targets) => targets,
            Err(error) => return error,
        };

        let Some(ipi) = ipi::get() else {
            return SbiRet::failed();
        };
        ipi.send(IpiEvent::Supervisor, targets)
            .map_or(SbiRet::failed(), |_| SbiRet::success(0))
    }
}

/// Validates the mask before yielding any target.
///
/// Stopped harts remain valid but are skipped.
/// Availability is checked as each hart is visited
/// and can change concurrently with hart lifecycle calls.
pub(super) fn target_harts(hart_mask: HartMask) -> Result<impl Iterator<Item = HartId>, SbiRet> {
    let (mask, base) = hart_mask.into_inner();
    if base != HartMask::IGNORE_MASK {
        // Reject an invalid mask before sending to any target.
        for bit in HartMask::from_mask_base(mask, 0) {
            let raw = base.checked_add(bit).ok_or_else(SbiRet::invalid_param)?;
            HartId::from_raw(raw).map_err(|_| SbiRet::invalid_param())?;
        }
    }

    Ok(HartId::all().filter(move |hart| {
        hart_mask.has_bit(hart.as_usize())
            && hart_privilege_checked(*hart)
            && hart::can_receive_ipi(*hart)
    }))
}
