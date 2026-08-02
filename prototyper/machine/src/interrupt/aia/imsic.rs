//! IMSIC file operations for a layout already validated by firmware.

use alloc::vec::Vec;
use riscv_aia::imsic::{FileConfig, MachineIndirectCsr, Xlen, initialize_machine_file};
use riscv_aia::peripheral::imsic::msi;

use crate::hart::{IpiDevice, IpiError, Notification};
use crate::trap::probe::{ExpectedResult, probe_csr, swap_csr};
use crate::{IoMem, io_fence};

use super::ImsicFile;

const MISELECT: u16 = 0x350;
const MIREG: u16 = 0x351;
const MTOPEI: u16 = 0x35c;

pub(super) struct Imsic {
    windows: Vec<IoMem>,
    files: Vec<ImsicFile>,
    config: FileConfig,
}

impl Imsic {
    pub(super) fn new(windows: Vec<IoMem>, files: Vec<ImsicFile>, config: FileConfig) -> Self {
        Self {
            windows,
            files,
            config,
        }
    }

    fn initialize_current_file(&self) -> Result<(), IpiError> {
        initialize_machine_file(&MachineCsr, xlen(), self.config).map_err(|_| IpiError::Failed)
    }
}

impl IpiDevice for Imsic {
    fn prepare_current_hart(&self) -> Result<(), IpiError> {
        let hart_id = current_hart_id();
        self.files
            .iter()
            .any(|file| file.hart_id == hart_id)
            .then_some(())
            .ok_or(IpiError::InvalidHart)?;
        self.initialize_current_file()
    }

    fn notify(&self, hart_id: usize) {
        let Some(file) = self.files.iter().find(|file| file.hart_id == hart_id) else {
            return;
        };
        let Some(window) = self
            .windows
            .iter()
            .find(|window| window.covers(file.address, 4))
        else {
            return;
        };
        let Some(offset) = file.address.checked_sub(window.range().start) else {
            return;
        };
        io_fence();
        let _ = window.write_once(
            offset,
            msi::encode_le(self.config.notification_identity().number()).to_le(),
        );
        io_fence();
    }

    fn claim(&self, hart_id: usize) {
        if hart_id != current_hart_id() {
            return;
        }
        // SAFETY: current-hart preparation enabled only this firmware IID.
        let _ = unsafe { swap_csr::<MTOPEI>(0) };
        io_fence();
    }

    fn notification(&self) -> Notification {
        Notification::External
    }
}

struct MachineCsr;

impl MachineIndirectCsr for MachineCsr {
    type Error = ();

    fn swap_select(&self, value: usize) -> Result<usize, Self::Error> {
        // SAFETY: this private adapter accesses only the fixed machine IMSIC selector CSR.
        match unsafe { swap_csr::<MISELECT>(value) } {
            ExpectedResult::Value(previous) => Ok(previous),
            ExpectedResult::Fault(_) | ExpectedResult::Busy | ExpectedResult::Unavailable => {
                Err(())
            }
        }
    }

    fn read_indirect(&self) -> Result<usize, Self::Error> {
        // SAFETY: the library selects only fixed IMSIC registers through this adapter.
        match unsafe { probe_csr::<MIREG>() } {
            ExpectedResult::Value(value) => Ok(value),
            ExpectedResult::Fault(_) | ExpectedResult::Busy | ExpectedResult::Unavailable => {
                Err(())
            }
        }
    }

    fn swap_indirect(&self, value: usize) -> Result<usize, Self::Error> {
        // SAFETY: the library selects only fixed IMSIC registers through this adapter.
        match unsafe { swap_csr::<MIREG>(value) } {
            ExpectedResult::Value(previous) => Ok(previous),
            ExpectedResult::Fault(_) | ExpectedResult::Busy | ExpectedResult::Unavailable => {
                Err(())
            }
        }
    }
}

fn xlen() -> Xlen {
    match usize::BITS {
        32 => Xlen::X32,
        64 => Xlen::X64,
        _ => unreachable!(),
    }
}

fn current_hart_id() -> usize {
    let value;
    // SAFETY: mhartid is a mandatory read-only machine CSR.
    unsafe {
        core::arch::asm!("csrr {value}, mhartid", value = out(reg) value, options(nomem, nostack))
    };
    value
}
