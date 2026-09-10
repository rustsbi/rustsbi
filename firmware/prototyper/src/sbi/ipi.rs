//! Inter-processor interrupts and firmware IPI delivery.
//!
//! # References
//!
//! - Specification: [RISC-V SBI IPI extension](https://docs.riscv.org/reference/sbi/v3.0/ext-ipi.html) —
//!   hart-mask handling and IPI delivery semantics.

#![forbid(unsafe_code)]

use super::pmu::pmu_firmware_counter_increment;
use crate::cfg::NUM_HART_MAX;
use crate::driver::{IpiBackend, IpiError, IpiRequest};
use crate::riscv::current_hartid;
use crate::sbi::hsm::remote_hsm;
use crate::sbi::rfence;
use crate::sbi::trap_stack::hart_local;
use alloc::{boxed::Box, vec::Vec};
use core::sync::atomic::{
    Ordering::{Acquire, Relaxed, Release},
    fence,
};
use rustsbi::{HartMask, SbiRet};
use sbi_spec::pmu::firmware_event;
use spin::Mutex;

/// IPI type for supervisor software interrupt.
pub(crate) const IPI_TYPE_SSOFT: u8 = 1 << 0;
/// IPI type for memory fence operations.
pub(crate) const IPI_TYPE_FENCE: u8 = 1 << 1;

/// SBI IPI extension.
pub struct SbiIpi {
    /// IPI device: CLINT `msip` registers or IMSIC MSI files.
    device: Mutex<Box<dyn IpiBackend + Send>>,
    /// Maximum hart ID in the system.
    pub max_hart_id: usize,
}

impl rustsbi::Ipi for SbiIpi {
    /// Sends IPIs to the specified harts.
    #[inline]
    fn send_ipi(&self, hart_mask: rustsbi::HartMask) -> SbiRet {
        pmu_firmware_counter_increment(firmware_event::IPI_SENT);
        let requests = match target_requests(hart_mask, self.max_hart_id) {
            Ok(requests) => requests,
            Err(error) => return error,
        };

        for req in requests {
            for hart_id in req.harts() {
                set_ipi_type(hart_id, IPI_TYPE_SSOFT);
            }
            // Always signal: pending bits can remain after a failed send.
            fence(Release);
            if self.device.lock().send_ipi(req).is_err() {
                return SbiRet::failed();
            }
        }

        SbiRet::success(0)
    }
}

impl SbiIpi {
    /// Creates a new SBI IPI extension.
    #[inline]
    pub(crate) fn new(device: Box<dyn IpiBackend + Send>, max_hart_id: usize) -> Self {
        Self {
            device: Mutex::new(device),
            max_hart_id,
        }
    }

    /// Sends an IPI carrying a remote fence operation.
    pub fn send_ipi_by_fence(
        &self,
        hart_mask: rustsbi::HartMask,
        ctx: rfence::RFenceContext,
    ) -> SbiRet {
        let current_hart = current_hartid();
        let requests = match target_requests(hart_mask, self.max_hart_id) {
            Ok(requests) => requests,
            Err(error) => return error,
        };
        let local = rfence::local_rfence().unwrap();
        let mut result = SbiRet::success(0);

        for hart_id in requests.into_iter().flat_map(IpiRequest::harts) {
            let remote = rfence::remote_rfence(hart_id).unwrap();
            local.add();
            remote.set(ctx);
            if hart_id == current_hart {
                continue;
            }

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
            rfence::rfence_single_handler();
        }

        result
    }

    /// Sends a firmware IPI to a hart.
    #[inline]
    pub(crate) fn send_ipi(&self, hart_id: usize) -> Result<(), IpiError> {
        // Publish the pending IPI type before signaling the target device.
        fence(Release);
        self.device.lock().send_ipi(IpiRequest {
            hart_mask: 1,
            hart_mask_base: hart_id,
        })
    }

    /// Clears the specified hart's firmware IPI register.
    #[inline]
    pub(crate) fn clear_ipi(&self, hart_id: usize) -> Result<(), IpiError> {
        self.device.lock().clear_ipi(hart_id)
    }

    /// Reports whether IMSIC was selected after validation.
    #[inline]
    pub(crate) fn uses_imsic(&self) -> bool {
        self.device.lock().is_imsic()
    }
}

/// Marks `event_id` pending for `hart_id`, returning the previous set.
pub fn set_ipi_type(hart_id: usize, event_id: u8) -> u8 {
    hart_local(hart_id).ipi_type.fetch_or(event_id, Relaxed)
}

/// Takes and clears the current hart's pending IPI types.
pub fn get_and_reset_ipi_type() -> u8 {
    hart_local(current_hartid()).ipi_type.swap(0, Acquire)
}

/// Clears the current hart's pending firmware IPI.
#[inline]
pub fn claim_ipi() {
    match crate::sbi::ipi() {
        Some(ipi) => ipi
            .clear_ipi(current_hartid())
            .expect("BUG: validated IPI backend could not clear the current hart"),
        None => error!("SBI or IPI device not initialized"),
    }
}

/// Initializes the SBI IPI extension from the selected device.
pub(crate) fn init(ipi: Box<dyn IpiBackend + Send>) -> SbiIpi {
    // Include DT-enabled harts even if they have not entered firmware yet.
    let max_hart_id = crate::platform::board_info()
        .enabled_harts
        .iter()
        .rposition(|enabled| *enabled)
        .unwrap_or(NUM_HART_MAX - 1);

    SbiIpi::new(ipi, max_hart_id)
}

/// Reports whether the selected IPI device is IMSIC.
pub(crate) fn uses_imsic() -> bool {
    crate::sbi::ipi().is_some_and(SbiIpi::uses_imsic)
}

fn target_requests(hart_mask: HartMask, max_hart_id: usize) -> Result<Vec<IpiRequest>, SbiRet> {
    let enabled = crate::platform::enabled_harts().unwrap_or([false; NUM_HART_MAX]);
    let available = |hart_id: usize| {
        hart_id <= max_hart_id
            && enabled.get(hart_id).copied().unwrap_or(false)
            && remote_hsm(hart_id).is_some_and(|hsm| hsm.allow_ipi())
    };
    let (mask, base) = hart_mask.into_inner();
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
        // Validate every selected hart before any event or backend is touched.
        for bit in 0..usize::BITS {
            if mask & (1 << bit) == 0 {
                continue;
            }
            let hart_id = base
                .checked_add(bit as usize)
                .ok_or_else(SbiRet::invalid_param)?;
            if !available(hart_id) {
                return Err(SbiRet::invalid_param());
            }
        }
        requests.push(IpiRequest {
            hart_mask: mask,
            hart_mask_base: base,
        });
    }
    Ok(requests)
}
