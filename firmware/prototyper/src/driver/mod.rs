//! Device drivers.
//!
//! Platform devices are identified by Devicetree `compatible` strings
//! (e.g. `"sifive,clint0"`). Each driver declares the strings it supports
//! in a `*_COMPATIBLES` table; platform discovery matches a table once and
//! records the selected device kind, which the `from_board` factories
//! construct devices from.

mod aia;
mod clint;
mod console;
mod reset;

use alloc::boxed::Box;

use crate::platform::BoardInfo;
use crate::riscv::csr::{mie, mip, stimecmp};
use crate::riscv::current_hartid;

pub(crate) use aia::{IMSIC_COMPATIBLES, IMSIC_FILE_SPAN, per_hart_init};
pub(crate) use clint::ClintKind;
pub(crate) use console::{ConsoleDevice, ConsoleKind};

pub(crate) use reset::{
    P1_PMIC_COMPATIBLES, PMIC_I2C_COMPATIBLES, ResetDevice, SIFIVE_TEST_COMPATIBLES,
};

/// Timer operations used by the SBI timer extension.
pub(crate) trait TimerDevice: Send {
    /// Reads the platform time counter.
    fn read_time(&self) -> u64;

    /// Programs the timer comparison value for `hart_idx`.
    fn set_timer(&self, hart_idx: usize, value: u64);
}

/// Inter-processor interrupt operations implemented by a platform device.
pub(crate) trait IpiDevice: Send {
    /// Signals a firmware IPI to `hart_idx`.
    fn send_ipi(&self, hart_idx: usize);

    /// Clears the current hart's pending firmware IPI.
    fn clear_ipi(&self);

    /// Reports whether firmware IPIs arrive through an IMSIC interrupt file.
    fn is_imsic(&self) -> bool {
        false
    }
}

/// Timer and IPI capabilities selected for the platform.
pub(crate) struct InterruptDevices {
    pub(crate) timer: Box<dyn TimerDevice>,
    pub(crate) ipi: Box<dyn IpiDevice>,
}

/// Timer implementation using the Sstc `stimecmp` CSR.
struct SstcTimer;

impl TimerDevice for SstcTimer {
    #[inline(always)]
    fn read_time(&self) -> u64 {
        riscv::register::time::read64()
    }

    #[inline(always)]
    fn set_timer(&self, hart_idx: usize, value: u64) {
        if hart_idx == current_hartid() {
            stimecmp::set(value);
            if value == u64::MAX {
                mip::clear_stimer();
                mie::clear_mtimer();
            }
        }
    }
}

pub(crate) fn console_device(board: &BoardInfo) -> Option<Box<dyn ConsoleDevice>> {
    console::from_board(board)
}

pub(crate) fn reset_device(board: &BoardInfo) -> Option<Box<dyn ResetDevice>> {
    reset::from_board(board)
}

pub(crate) fn interrupt_devices(board: &BoardInfo) -> Option<InterruptDevices> {
    if let Some(aia_info) = board.aia.as_ref()
        && let Some(devices) = aia::from_board(board, aia_info)
    {
        return Some(devices);
    }
    clint::from_board(board)
}
