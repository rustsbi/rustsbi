//! Inter-processor interrupts and firmware IPI delivery.
//!
//! # References
//!
//! - Specification: [RISC-V SBI IPI extension](https://docs.riscv.org/reference/sbi/v3.0/ext-ipi.html) —
//!   hart-mask handling and IPI delivery semantics.

#![forbid(unsafe_code)]

use super::pmu::pmu_firmware_counter_increment;
use crate::cfg::NUM_HART_MAX;
use crate::driver::IpiDevice;
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
    device: Mutex<Box<dyn IpiDevice>>,
    /// Maximum hart ID in the system.
    pub max_hart_id: usize,
}

impl rustsbi::Ipi for SbiIpi {
    /// Sends IPIs to the specified harts.
    #[inline]
    fn send_ipi(&self, hart_mask: rustsbi::HartMask) -> SbiRet {
        pmu_firmware_counter_increment(firmware_event::IPI_SENT);
        let mut deliver_harts = Vec::new();

        for hart_id in target_harts(hart_mask, self.max_hart_id) {
            // Reject targets that are out of range, absent, disabled, or in a
            // state that does not accept IPIs; the delivery loop below
            // assumes every collected hart passed these checks.
            if hart_id > self.max_hart_id {
                return SbiRet::invalid_param();
            }

            let Some(hsm) = remote_hsm(hart_id) else {
                return SbiRet::invalid_param();
            };

            if crate::platform::enabled_harts()
                .is_none_or(|list| list.get(hart_id).is_none_or(|res| !(*res)))
            {
                return SbiRet::invalid_param();
            }

            if !hsm.allow_ipi() {
                return SbiRet::invalid_param();
            }

            deliver_harts.push(hart_id);
        }

        for hart_id in deliver_harts {
            if set_ipi_type(hart_id, IPI_TYPE_SSOFT) == 0 {
                self.send_ipi(hart_id);
            }
        }

        SbiRet::success(0)
    }
}

impl SbiIpi {
    /// Creates a new SBI IPI extension.
    #[inline]
    pub(crate) fn new(device: Box<dyn IpiDevice>, max_hart_id: usize) -> Self {
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
        let mut deliver_harts = Vec::new();

        for hart_id in target_harts(hart_mask, self.max_hart_id) {
            // Reject targets that are out of range, absent, disabled, or in a
            // state that does not accept IPIs; the delivery loop below
            // assumes every collected hart passed these checks.
            if hart_id > self.max_hart_id {
                return SbiRet::invalid_param();
            }

            let Some(hsm) = remote_hsm(hart_id) else {
                return SbiRet::invalid_param();
            };

            if crate::platform::enabled_harts()
                .is_none_or(|list| list.get(hart_id).is_none_or(|res| !(*res)))
            {
                return SbiRet::invalid_param();
            }

            if !hsm.allow_ipi() {
                return SbiRet::invalid_param();
            }

            deliver_harts.push(hart_id);
        }

        for hart_id in deliver_harts {
            if let Some(remote) = rfence::remote_rfence(hart_id) {
                if let Some(local) = rfence::local_rfence() {
                    local.add();
                }
                remote.set(ctx);
                if hart_id != current_hart {
                    let old_ipi_type = set_ipi_type(hart_id, IPI_TYPE_FENCE);
                    if old_ipi_type == 0 {
                        self.send_ipi(hart_id);
                    }
                }
            }
        }

        while !rfence::local_rfence().unwrap().is_sync() {
            rfence::rfence_single_handler();
        }

        SbiRet::success(0)
    }

    /// Sends a firmware IPI to a hart.
    #[inline]
    pub(crate) fn send_ipi(&self, hart_id: usize) {
        // Publish the pending IPI type before signaling the target device.
        fence(Release);
        self.device.lock().send_ipi(hart_id);
    }

    /// Clears the current hart's firmware IPI.
    #[inline]
    pub(crate) fn clear_ipi(&self) {
        self.device.lock().clear_ipi();
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
        Some(ipi) => ipi.clear_ipi(),
        None => error!("SBI or IPI device not initialized"),
    }
}

/// Initializes the SBI IPI extension from the selected device.
pub(crate) fn init(ipi: Box<dyn IpiDevice>) -> SbiIpi {
    let max_hart_id = crate::platform::enabled_harts()
        .as_ref()
        .and_then(|hart_list| hart_list.iter().rposition(|enabled| *enabled))
        .unwrap_or(NUM_HART_MAX - 1);

    SbiIpi::new(ipi, max_hart_id)
}

/// Reports whether the selected IPI device is IMSIC.
pub(crate) fn uses_imsic() -> bool {
    crate::sbi::ipi().is_some_and(SbiIpi::uses_imsic)
}

fn target_harts(hart_mask: HartMask, max_hart_id: usize) -> Vec<usize> {
    let (_mask, mask_base) = hart_mask.into_inner();
    if mask_base == usize::MAX {
        (0..=max_hart_id).collect()
    } else {
        hart_mask.into_iter().collect()
    }
}
