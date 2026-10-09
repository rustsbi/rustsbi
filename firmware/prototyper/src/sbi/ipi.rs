//! Inter-processor interrupts and firmware IPI delivery.
//!
//! # References
//!
//! - Specification: [RISC-V SBI IPI extension](https://docs.riscv.org/reference/sbi/v3.0/ext-ipi.html) —
//!   hart-mask handling and IPI delivery semantics.

#![forbid(unsafe_code)]

use super::pmu::pmu_firmware_counter_increment;
use crate::driver::ipi::IpiDevice;
use crate::driver::{IpiError, IpiRequest};
use crate::sbi::hart_local::hart_local;
use crate::sbi::rfence;
use alloc::vec::Vec;
use core::sync::atomic::Ordering::{Acquire, Release};
use runtime::hart::{self, HartId};
use runtime::rustsbi::{HartMask, SbiRet};
use sbi_spec::pmu::firmware_event;

/// IPI type for supervisor software interrupt.
pub(crate) const IPI_TYPE_SSOFT: u8 = 1 << 0;
/// IPI type for memory fence operations.
pub(crate) const IPI_TYPE_FENCE: u8 = 1 << 1;

/// SBI IPI extension.
pub struct SbiIpi {
    device: &'static IpiDevice,
}

/// Delivers SBI work after Runtime acknowledges the device interrupt.
struct RuntimeIpiHandler;
static RUNTIME_IPI_HANDLER: RuntimeIpiHandler = RuntimeIpiHandler;

impl runtime::rustsbi::Ipi for SbiIpi {
    /// Sends IPIs to the specified harts.
    #[inline]
    fn send_ipi(&self, hart_mask: runtime::rustsbi::HartMask) -> SbiRet {
        pmu_firmware_counter_increment(firmware_event::IPI_SENT);
        let requests = match target_requests(hart_mask) {
            Ok(requests) => requests,
            Err(error) => return error,
        };

        for req in requests {
            for hart_id in req.harts() {
                set_ipi_type(hart_id, IPI_TYPE_SSOFT);
            }
            // Always signal: pending bits can remain after a failed send.
            if self.device.send_ipi(req).is_err() {
                return SbiRet::failed();
            }
        }

        SbiRet::success(0)
    }
}

impl SbiIpi {
    /// Adapts a published machine IPI device to the SBI IPI extension.
    pub(crate) fn new(device: &'static IpiDevice) -> Self {
        Self { device }
    }
    /// Marks remote fence work pending and signals its target hart.
    pub(super) fn send_fence_ipi(&self, hart: HartId) -> Result<(), IpiError> {
        set_ipi_type(hart.as_usize(), IPI_TYPE_FENCE);
        self.device.send_ipi(IpiRequest::for_hart(hart.as_usize()))
    }
}

impl runtime::ipi::IpiHandler for RuntimeIpiHandler {
    fn deliver_current(&self) {
        let ipi_type = get_and_reset_ipi_type();
        if (ipi_type & IPI_TYPE_SSOFT) != 0 {
            pmu_firmware_counter_increment(firmware_event::IPI_RECEIVED);
            runtime::csr::mip::set_supervisor_software();
        }
        if (ipi_type & IPI_TYPE_FENCE) != 0 {
            rfence::rfence_handler();
        }
    }
}

/// Returns the Runtime firmware-work adapter.
pub(crate) fn runtime_handler() -> &'static dyn runtime::ipi::IpiHandler {
    &RUNTIME_IPI_HANDLER
}

/// Marks `event_id` pending for `hart_id`, returning the previous set.
pub fn set_ipi_type(hart_id: usize, event_id: u8) -> u8 {
    hart_local(hart_id).ipi_type.fetch_or(event_id, Release)
}

/// Takes and clears the current hart's pending IPI types.
pub fn get_and_reset_ipi_type() -> u8 {
    // The device clear/claim must precede the pending-event read.
    crate::riscv::csr::fence::io_to_memory();
    let hart_id = HartId::current()
        .expect("BUG: current hart is not in the boot topology")
        .as_usize();
    hart_local(hart_id).ipi_type.swap(0, Acquire)
}

fn target_requests(hart_mask: HartMask) -> Result<impl Iterator<Item = IpiRequest>, SbiRet> {
    let available = |hart: HartId| {
        crate::platform::hart_privilege_checked(hart.as_usize()) && hart::can_receive_ipi(hart)
    };
    let (mask, base) = hart_mask.into_inner();
    let mut single = None;
    let mut requests: Vec<IpiRequest> = Vec::new();
    if base == usize::MAX {
        // Ignore mask and expand all available harts into ordinary windows.
        for hart in HartId::all().filter(|hart| available(*hart)) {
            let singleton = IpiRequest::for_hart(hart.as_usize());
            if !requests
                .last_mut()
                .is_some_and(|request| request.try_merge(singleton))
            {
                requests.push(singleton);
            }
        }
    } else if mask != 0 {
        // Validate platform assignment separately from transient HSM state.
        // Stopped harts remain valid targets but need no supervisor IPI.
        let mut active_mask = 0;
        for bit in HartMask::from_mask_base(mask, 0) {
            let hart_id = base.checked_add(bit).ok_or_else(SbiRet::invalid_param)?;
            let hart = HartId::from_raw(hart_id).map_err(|_| SbiRet::invalid_param())?;
            if available(hart) {
                active_mask |= 1 << bit;
            }
        }
        // An ordinary mask needs one window, without a heap allocation.
        single = (active_mask != 0).then_some(IpiRequest::from_mask_base(active_mask, base));
    }
    Ok(single.into_iter().chain(requests))
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
