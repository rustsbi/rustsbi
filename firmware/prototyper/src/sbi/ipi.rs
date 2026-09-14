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

    /// Sends an IPI carrying a remote fence operation.
    pub fn send_ipi_by_fence(
        &self,
        hart_mask: runtime::rustsbi::HartMask,
        ctx: rfence::RFenceContext,
    ) -> SbiRet {
        let current_hart = HartId::current()
            .expect("BUG: current hart exceeds Runtime capacity")
            .as_usize();
        let requests = match target_requests(hart_mask) {
            Ok(requests) => requests,
            Err(error) => return error,
        };
        let local = rfence::local_rfence().unwrap();
        let mut result = SbiRet::success(0);

        for hart_id in requests.flat_map(IpiRequest::harts) {
            // Improve performance if the RFence request runs on the local host.
            if hart_id == current_hart {
                rfence::rfence_local_handler(ctx);
                continue;
            }

            let remote = rfence::remote_rfence(hart_id).unwrap();
            local.add();
            remote.set(ctx);

            set_ipi_type(hart_id, IPI_TYPE_FENCE);
            if self.send_ipi(hart_id).is_ok() {
                continue;
            }
            // Cancel this source's queued request; a receiver that already
            // took it remains responsible for the acknowledgement.
            if remote.cancel(current_hart) {
                rfence::remote_rfence(current_hart).unwrap().sub();
            }
            result = SbiRet::failed();
            break;
        }

        // Complete previously submitted operations even if a later send failed.
        while !local.is_sync() {
            rfence::rfence_poll();
        }

        result
    }

    /// Sends a firmware IPI to a hart.
    #[inline]
    pub(crate) fn send_ipi(&self, hart_id: usize) -> Result<(), IpiError> {
        self.device.send_ipi(IpiRequest {
            hart_mask: 1,
            hart_mask_base: hart_id,
        })
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
        .expect("BUG: current hart exceeds Runtime capacity")
        .as_usize();
    hart_local(hart_id).ipi_type.swap(0, Acquire)
}

fn target_requests(hart_mask: HartMask) -> Result<impl Iterator<Item = IpiRequest>, SbiRet> {
    let enabled = &crate::platform::board_info().enabled_harts;
    let assigned = |hart_id: usize| {
        HartId::from_raw(hart_id).is_ok() && enabled.get(hart_id).copied().unwrap_or(false)
    };
    let available = |hart_id: usize| {
        let Ok(hart) = HartId::from_raw(hart_id) else {
            return false;
        };
        assigned(hart_id)
            && crate::platform::hart_privilege_checked(hart_id)
            && hart::can_receive_ipi(hart)
    };
    let (mask, base) = hart_mask.into_inner();
    let mut single = None;
    let mut requests = Vec::new();
    if base == usize::MAX {
        // Ignore mask and expand all available harts into ordinary windows.
        for (window, harts) in enabled.chunks(usize::BITS as usize).enumerate() {
            let base = window * usize::BITS as usize;
            let mask = (0..harts.len()).fold(0, |mask, bit| {
                mask | (usize::from(available(base + bit)) << bit)
            });
            if mask != 0 {
                requests.push(IpiRequest {
                    hart_mask: mask,
                    hart_mask_base: base,
                });
            }
        }
    } else if mask != 0 {
        // Validate platform assignment separately from transient HSM state.
        // Stopped harts remain valid targets but need no supervisor IPI.
        let mut active_mask = 0;
        for bit in HartMask::from_mask_base(mask, 0) {
            let hart_id = base.checked_add(bit).ok_or_else(SbiRet::invalid_param)?;
            if !assigned(hart_id) {
                return Err(SbiRet::invalid_param());
            }
            if available(hart_id) {
                active_mask |= 1 << bit;
            }
        }
        // An ordinary mask needs one window, without a heap allocation
        single = (active_mask != 0).then_some(IpiRequest {
            hart_mask: active_mask,
            hart_mask_base: base,
        });
    }
    // Chains single request with batch requests, which improves performance
    // for single hart request.
    Ok(single.into_iter().chain(requests))
}
