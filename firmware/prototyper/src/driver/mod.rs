//! Device drivers.
//!
//! Platform devices are identified by Devicetree `compatible` strings
//! (e.g. `"sifive,clint0"`). Each driver declares the strings it supports
//! in a `*_COMPATIBLES` table. Platform discovery records the matching
//! register descriptions. [`bind_devices`] binds those descriptions to Runtime MMIO
//! windows and constructs the platform devices.

#![forbid(unsafe_code)]

mod aia;
mod clint;
mod console;
mod reset;

use alloc::boxed::Box;

use runtime::memory::MemoryRegistry;

use crate::platform::BoardInfo;
use crate::riscv::csr::{mie, mip, stimecmp};
use crate::riscv::current_hartid;

pub(crate) use aia::{IMSIC_COMPATIBLES, IMSIC_FILE_SPAN, initialize_hart_imsic};
pub(crate) use clint::ClintKind;
pub(crate) use console::{ConsoleDevice, ConsoleKind};

pub(crate) use reset::{
    I2cAddress, P1_PMIC_COMPATIBLES, PMIC_I2C_COMPATIBLES, ResetDevice, SIFIVE_TEST_COMPATIBLES,
};

/// Platform devices constructed from the discovered hardware description.
pub(crate) struct Devices {
    pub(crate) interrupts: Option<InterruptDevices>,
    pub(crate) console: Option<Box<dyn ConsoleDevice>>,
    pub(crate) reset: Option<Box<dyn ResetDevice>>,
}

impl Devices {
    /// Returns whether firmware IPIs use IMSIC interrupt files.
    pub(crate) fn uses_imsic(&self) -> bool {
        self.interrupts
            .as_ref()
            .is_some_and(|devices| devices.ipi.is_imsic())
    }
}

/// Timer and IPI devices selected for the platform.
pub(crate) struct InterruptDevices {
    pub(crate) timer: Box<dyn TimerDevice>,
    pub(crate) ipi: Box<dyn IpiDevice>,
}

/// Timer operations used by the SBI timer extension.
pub(crate) trait TimerDevice: Send {
    /// Reads the platform time counter.
    fn read_time(&self) -> u64;

    /// Programs the timer comparison value for `hart_id`.
    fn set_timer(&self, hart_id: usize, value: u64);
}

/// Inter-processor interrupt operations implemented by a platform device.
pub(crate) trait IpiDevice: Send {
    /// Signals a firmware IPI to `hart_id`.
    fn send_ipi(&self, hart_id: usize);

    /// Clears the current hart's pending firmware IPI.
    fn clear_ipi(&self);

    /// Reports whether firmware IPIs arrive through an IMSIC interrupt file.
    fn is_imsic(&self) -> bool {
        false
    }
}

/// Timer implementation using the Sstc `stimecmp` CSR.
struct SstcTimer;

impl TimerDevice for SstcTimer {
    #[inline(always)]
    fn read_time(&self) -> u64 {
        riscv::register::time::read64()
    }

    #[inline(always)]
    fn set_timer(&self, hart_id: usize, value: u64) {
        if hart_id == current_hartid() {
            stimecmp::set(value);
            if value == u64::MAX {
                mip::clear_stimer();
                mie::clear_mtimer();
            }
        }
    }
}

fn bind_interrupts(
    board: &BoardInfo,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Option<InterruptDevices>> {
    if let Some(imsic) = board.imsic.as_ref().filter(|_| aia::is_eligible(board)) {
        let aplic_config = if board.is_qemu_virt() {
            Some(crate::platform::qemu_aplic::QemuAplicConfig::new(
                board.machine_aplic.ok_or(runtime::Error::InvalidArgs)?,
                imsic.layout.machine_base,
                imsic.layout.hart_index_bits,
            )?)
        } else {
            warn!("AIA: skipping QEMU virt M-APLIC setup on '{}'", board.model);
            None
        };
        return aia::bind(imsic, aplic_config, memory).map(Some);
    }
    let Some(&(registers, kind)) = board.clint.as_ref() else {
        return Ok(None);
    };
    clint::bind(registers, kind, memory).map(Some)
}

/// Binds all devices selected during platform discovery.
pub(crate) fn bind_devices(
    board: &BoardInfo,
    memory: &mut MemoryRegistry,
) -> runtime::Result<Devices> {
    Ok(Devices {
        interrupts: bind_interrupts(board, memory)?,
        console: console::bind(board, memory)?,
        reset: reset::bind(board, memory)?,
    })
}
