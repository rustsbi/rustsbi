#![forbid(unsafe_code)]

//! SBI timer and IPI extensions.

use super::pmu::pmu_firmware_counter_increment;
use crate::cfg::NUM_HART_MAX;
use crate::driver::{InterruptBackend, IpiSender, TimerDevice};
use crate::platform::BoardInfo;
use crate::riscv::csr::{mie, mip, stimecmp};
use crate::riscv::current_hartid;
use crate::sbi::features::{Extension, hart_extension_probe};
use crate::sbi::hsm::remote_hsm;
use crate::sbi::rfence;
use crate::sbi::trap_stack::hart_local;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::Ordering::Relaxed;
use rustsbi::{HartMask, SbiRet};
use sbi_spec::pmu::firmware_event;
use spin::Mutex;

/// IPI type for supervisor software interrupt.
pub(crate) const IPI_TYPE_SSOFT: u8 = 1 << 0;
/// IPI type for memory fence operations.
pub(crate) const IPI_TYPE_FENCE: u8 = 1 << 1;

/// SBI timer and IPI service.
pub struct SbiIpi {
    /// IPI device: CLINT `msip` registers or IMSIC MSI files.
    ipi: Mutex<Box<dyn IpiSender>>,
    /// Timer device: CLINT `mtimecmp` registers or the Sstc `stimecmp` CSR.
    timer: Mutex<Box<dyn TimerDevice>>,
    /// Backend that passed validation during construction.
    backend: InterruptBackend,
    /// Maximum hart ID in the system.
    pub max_hart_id: usize,
}

impl rustsbi::Timer for SbiIpi {
    /// Sets the timer for the current hart.
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        pmu_firmware_counter_increment(firmware_event::SET_TIMER);
        let hart_id = current_hartid();

        if hart_extension_probe(hart_id, Extension::Sstc) {
            stimecmp::set(stime_value);
        } else {
            self.set_timer_for_hart(hart_id, stime_value);
            mip::clear_stimer();
            mie::set_mtimer();
        }
    }
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

            if crate::platform::cpu_enabled()
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
    /// Creates a new SBI timer and IPI service.
    #[inline]
    pub(crate) fn new(
        ipi: Mutex<Box<dyn IpiSender>>,
        timer: Mutex<Box<dyn TimerDevice>>,
        backend: InterruptBackend,
        max_hart_id: usize,
    ) -> Self {
        Self {
            ipi,
            timer,
            backend,
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

            if crate::platform::cpu_enabled()
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

    /// Gets the lower 32 bits of machine time.
    #[inline]
    pub fn get_time(&self) -> usize {
        self.timer.lock().read_time() as usize
    }

    /// Gets the upper 32 bits of machine time.
    #[inline]
    pub fn get_timeh(&self) -> usize {
        (self.timer.lock().read_time() >> 32) as usize
    }

    /// Sends a firmware IPI to a hart.
    #[inline]
    pub(crate) fn send_ipi(&self, hart_idx: usize) {
        self.ipi.lock().send_ipi(hart_idx);
    }

    /// Clears the current hart's firmware IPI.
    #[inline]
    pub(crate) fn clear_ipi(&self) {
        self.ipi.lock().clear_ipi();
    }

    /// Programs a hart's timer comparison value.
    #[inline]
    fn set_timer_for_hart(&self, hart_idx: usize, value: u64) {
        self.timer.lock().set_timer(hart_idx, value);
    }

    /// Reports whether IMSIC was selected after validation.
    #[inline]
    pub(crate) fn uses_imsic(&self) -> bool {
        self.backend == InterruptBackend::Imsic
    }

    /// Clears all pending interrupts for the current hart.
    #[inline]
    pub fn clear(&self) {
        let hart_id = current_hartid();
        self.ipi.lock().clear_ipi();
        self.timer.lock().set_timer(hart_id, u64::MAX);
    }
}

/// Marks `event_id` pending for `hart_id`, returning the previous set.
pub fn set_ipi_type(hart_id: usize, event_id: u8) -> u8 {
    hart_local(hart_id).ipi_type.fetch_or(event_id, Relaxed)
}

/// Takes and clears the current hart's pending IPI types.
pub fn get_and_reset_ipi_type() -> u8 {
    hart_local(current_hartid()).ipi_type.swap(0, Relaxed)
}

/// Clears the current hart's pending firmware IPI.
#[inline]
pub fn claim_ipi() {
    match crate::sbi::ipi() {
        Some(ipi) => ipi.clear_ipi(),
        None => error!("SBI or IPI device not initialized"),
    }
}

/// Cancels the current hart's machine timer interrupt.
#[inline]
pub fn clear_mtime() {
    match crate::sbi::ipi() {
        Some(ipi) => ipi.set_timer_for_hart(current_hartid(), u64::MAX),
        None => error!("SBI or IPI device not initialized"),
    }
}

/// Clears all pending interrupts for the current hart.
#[inline]
pub fn clear_all() {
    match crate::sbi::ipi() {
        Some(ipi) => ipi.clear(),
        None => error!("SBI or IPI device not initialized"),
    }
}

/// Initializes the SBI IPI extension from the discovered board info
/// (AIA probe with CLINT fallback).
pub(crate) fn init(board: &BoardInfo) -> Option<SbiIpi> {
    let max_hart_id = crate::platform::cpu_enabled()
        .as_ref()
        .and_then(|hart_list| hart_list.iter().rposition(|enabled| *enabled))
        .unwrap_or(NUM_HART_MAX - 1);

    let devices = crate::driver::interrupt_devices(board)?;
    Some(SbiIpi::new(
        Mutex::new(devices.ipi),
        Mutex::new(devices.timer),
        devices.backend,
        max_hart_id,
    ))
}

/// Reports whether the selected interrupt backend is IMSIC.
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
