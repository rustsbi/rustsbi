use super::pmu::pmu_firmware_counter_increment;
use crate::platform::PLATFORM;
use crate::riscv::csr::stimecmp;
use crate::riscv::current_hartid;
use crate::sbi::features::{Extension, hart_extension_probe};
use crate::sbi::hsm::remote_hsm;
use crate::sbi::rfence;
use crate::sbi::trap_stack::hart_context;
use alloc::boxed::Box;
use core::sync::atomic::Ordering::Relaxed;
use rustsbi::{HartMask, SbiRet};
use sbi_spec::pmu::firmware_event;
use spin::Mutex;

/// IPI type for supervisor software interrupt.
pub(crate) const IPI_TYPE_SSOFT: u8 = 1 << 0;
/// IPI type for memory fence operations.
pub(crate) const IPI_TYPE_FENCE: u8 = 1 << 1;

/// Trait defining interface for inter-processor interrupt device
#[allow(unused)]
pub trait IpiDevice {
    /// Read machine time value.
    fn read_mtime(&self) -> u64;
    /// Write machine time value.
    fn write_mtime(&self, val: u64);
    /// Read machine timer compare value for given hart.
    fn read_mtimecmp(&self, hart_idx: usize) -> u64;
    /// Write machine timer compare value for given hart.
    fn write_mtimecmp(&self, hart_idx: usize, val: u64);
    /// Read machine software interrupt pending bit for given hart.
    fn read_msip(&self, hart_idx: usize) -> bool;
    /// Set machine software interrupt pending bit for given hart.
    fn set_msip(&self, hart_idx: usize);
    /// Clear machine software interrupt pending bit for given hart.
    fn clear_msip(&self, hart_idx: usize);
}

/// SBI IPI implementation.
pub struct SbiIpi {
    /// Reference to atomic pointer to IPI device.
    pub ipi_dev: Mutex<Box<dyn IpiDevice>>,
    /// Maximum hart ID in the system
    pub max_hart_id: usize,
}

impl rustsbi::Timer for SbiIpi {
    /// Set timer value for current hart.
    #[inline]
    fn set_timer(&self, stime_value: u64) {
        pmu_firmware_counter_increment(firmware_event::SET_TIMER);
        let hart_id = current_hartid();
        let uses_sstc = hart_extension_probe(hart_id, Extension::Sstc);

        // Set timer value based on extension support.
        if uses_sstc {
            stimecmp::set(stime_value);
        } else {
            self.write_mtimecmp(hart_id, stime_value);
            unsafe {
                riscv::register::mip::clear_stimer();
            }
        }
        // Enable machine timer interrupt.
        unsafe {
            riscv::register::mie::set_mtimer();
        }
    }
}

impl rustsbi::Ipi for SbiIpi {
    /// Send IPI to specified harts.
    #[inline]
    fn send_ipi(&self, hart_mask: rustsbi::HartMask) -> SbiRet {
        pmu_firmware_counter_increment(firmware_event::IPI_SENT);
        let mut hart_mask = hart_mask;

        for hart_id in 0..=self.max_hart_id {
            if !hart_mask.has_bit(hart_id) {
                continue;
            }

            // There are 2 situation to return invalid_param:
            // 1. We can not get hsm, which usually means this hart_id is bigger than MAX_HART_ID.
            // 2. BOARD hasn't init or this hart_id is not enabled by device tree.
            // In the next loop, we'll assume that all of above situation will not happened and
            // directly send ipi.
            let Some(hsm) = remote_hsm(hart_id) else {
                return SbiRet::invalid_param();
            };

            if unsafe {
                PLATFORM
                    .info
                    .cpu_enabled
                    .is_none_or(|list| list.get(hart_id).is_none_or(|res| !(*res)))
            } {
                return SbiRet::invalid_param();
            }

            if !hsm.allow_ipi() {
                hart_mask = hart_mask_clear(hart_mask, hart_id);
            }
        }
        for hart_id in 0..=self.max_hart_id {
            if !hart_mask.has_bit(hart_id) {
                continue;
            }

            if set_ipi_type(hart_id, IPI_TYPE_SSOFT) == 0 {
                self.set_msip(hart_id);
            }
        }

        SbiRet::success(0)
    }
}

impl SbiIpi {
    /// Create new SBI IPI instance.
    #[inline]
    pub fn new(ipi_dev: Mutex<Box<dyn IpiDevice>>, max_hart_id: usize) -> Self {
        Self {
            ipi_dev,
            max_hart_id,
        }
    }

    /// Send IPI for remote fence operation.
    pub fn send_ipi_by_fence(
        &self,
        hart_mask: rustsbi::HartMask,
        ctx: rfence::RFenceContext,
    ) -> SbiRet {
        let current_hart = current_hartid();
        let mut hart_mask = hart_mask;

        for hart_id in 0..=self.max_hart_id {
            if !hart_mask.has_bit(hart_id) {
                continue;
            }

            // There are 2 situation to return invalid_param:
            // 1. We can not get hsm, which usually means this hart_id is bigger than MAX_HART_ID.
            // 2. BOARD hasn't init or this hart_id is not enabled by device tree.
            // In the next loop, we'll assume that all of above situation will not happened and
            // directly send ipi.
            let Some(hsm) = remote_hsm(hart_id) else {
                return SbiRet::invalid_param();
            };

            if unsafe {
                PLATFORM
                    .info
                    .cpu_enabled
                    .is_none_or(|list| list.get(hart_id).is_none_or(|res| !(*res)))
            } {
                return SbiRet::invalid_param();
            }

            if !hsm.allow_ipi() {
                hart_mask = hart_mask_clear(hart_mask, hart_id);
            }
        }

        // Send fence operations to target harts
        for hart_id in 0..=self.max_hart_id {
            if !hart_mask.has_bit(hart_id) {
                continue;
            }

            if let Some(remote) = rfence::remote_rfence(hart_id) {
                if let Some(local) = rfence::local_rfence() {
                    local.add();
                }
                remote.set(ctx);
                if hart_id != current_hart {
                    let old_ipi_type = set_ipi_type(hart_id, IPI_TYPE_FENCE);
                    if old_ipi_type == 0 {
                        self.set_msip(hart_id);
                    }
                }
            }
        }

        // Wait for all fence operations to complete
        while !rfence::local_rfence().unwrap().is_sync() {
            rfence::rfence_single_handler();
        }

        SbiRet::success(0)
    }

    /// Get lower 32 bits of machine time.
    #[inline]
    pub fn get_time(&self) -> usize {
        self.ipi_dev.lock().read_mtime() as usize
    }

    /// Get upper 32 bits of machine time.
    #[inline]
    pub fn get_timeh(&self) -> usize {
        (self.ipi_dev.lock().read_mtime() >> 32) as usize
    }

    /// Set machine software interrupt pending for hart.
    #[inline]
    pub fn set_msip(&self, hart_idx: usize) {
        self.ipi_dev.lock().set_msip(hart_idx);
    }

    /// Clear machine software interrupt pending for hart.
    #[inline]
    pub fn clear_msip(&self, hart_idx: usize) {
        self.ipi_dev.lock().clear_msip(hart_idx);
    }

    /// Write machine timer compare value for hart.
    #[inline]
    pub fn write_mtimecmp(&self, hart_idx: usize, val: u64) {
        self.ipi_dev.lock().write_mtimecmp(hart_idx, val);
    }

    /// Clear all pending interrupts for current hart.
    #[inline]
    pub fn clear(&self) {
        let hart_id = current_hartid();
        // Load ipi_dev once instead of twice
        let ipi_dev = self.ipi_dev.lock();
        ipi_dev.clear_msip(hart_id);
        ipi_dev.write_mtimecmp(hart_id, u64::MAX);
    }
}

/// Set IPI type for specified hart.
pub fn set_ipi_type(hart_id: usize, event_id: u8) -> u8 {
    hart_context(hart_id).ipi_type.fetch_or(event_id, Relaxed)
}

/// Get and reset IPI type for current hart.
pub fn get_and_reset_ipi_type() -> u8 {
    hart_context(current_hartid()).ipi_type.swap(0, Relaxed)
}

/// Clear machine software interrupt pending for current hart.
#[inline]
pub fn clear_msip() {
    match unsafe { PLATFORM.sbi.ipi.as_ref() } {
        Some(ipi) => ipi.clear_msip(current_hartid()),
        None => error!("SBI or IPI device not initialized"),
    }
}

/// Clear machine timer interrupt for current hart.
#[inline]
pub fn clear_mtime() {
    match unsafe { PLATFORM.sbi.ipi.as_ref() } {
        Some(ipi) => ipi.write_mtimecmp(current_hartid(), u64::MAX),
        None => error!("SBI or IPI device not initialized"),
    }
}

/// Clear all pending interrupts for current hart.
#[inline]
pub fn clear_all() {
    match unsafe { PLATFORM.sbi.ipi.as_ref() } {
        Some(ipi) => ipi.clear(),
        None => error!("SBI or IPI device not initialized"),
    }
}

pub fn hart_mask_clear(hart_mask: HartMask, hart_id: usize) -> HartMask {
    let (mask, mask_base) = hart_mask.into_inner();
    if mask_base == usize::MAX {
        return HartMask::from_mask_base(mask & (!(1 << hart_id)), 0);
    }
    let Some(idx) = hart_id.checked_sub(mask_base) else {
        return hart_mask;
    };
    if idx >= usize::BITS as usize {
        return hart_mask;
    }
    HartMask::from_mask_base(mask & (!(1 << hart_id)), mask_base)
}
