//! IMSIC file operations for a layout already validated by firmware.

use alloc::vec::Vec;

use crate::hart::{IpiDevice, IpiError, Notification};
use crate::trap::probe::{ExpectedResult, probe_csr, swap_csr};
use crate::{IoMem, io_fence};

use super::ImsicFile;

const MISELECT: u16 = 0x350;
const MIREG: u16 = 0x351;
const MTOPEI: u16 = 0x35c;
const EIDELIVERY: usize = 0x70;
const EITHRESHOLD: usize = 0x72;
const EIP_BASE: usize = 0x80;
const EIE_BASE: usize = 0xc0;

pub(super) struct Imsic {
    windows: Vec<IoMem>,
    files: Vec<ImsicFile>,
    interrupt_identity_count: u16,
    notification_identity: u16,
}

impl Imsic {
    pub(super) fn new(
        windows: Vec<IoMem>,
        files: Vec<ImsicFile>,
        interrupt_identity_count: u16,
        notification_identity: u16,
    ) -> Self {
        Self {
            windows,
            files,
            interrupt_identity_count,
            notification_identity,
        }
    }

    fn initialize_current_file(&self) -> Result<(), IpiError> {
        // SAFETY: selectors are fixed IMSIC registers and the identity count
        // was checked before this device was constructed.
        unsafe {
            indirect_write(EIDELIVERY, 0)?;
            indirect_write(EITHRESHOLD, 0)?;
            let word_bits = usize::BITS as usize;
            let word_count = usize::from(self.interrupt_identity_count).div_ceil(word_bits);
            let stride = if usize::BITS == 64 { 2 } else { 1 };
            for word in 0..word_count {
                indirect_write(EIP_BASE + word * stride, 0)?;
                indirect_write(EIE_BASE + word * stride, 0)?;
            }
            let iid = usize::from(self.notification_identity);
            indirect_write(
                EIE_BASE + (iid / word_bits) * stride,
                1usize << (iid % word_bits),
            )?;
            indirect_write(EIDELIVERY, 1)?;
        }
        Ok(())
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
        let _ = window.write_once(offset, u32::from(self.notification_identity).to_le());
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

unsafe fn indirect_write(selector: usize, value: usize) -> Result<(), IpiError> {
    // SAFETY: the caller supplies fixed selectors selected by this module.
    let original = match unsafe { swap_csr::<MISELECT>(selector) } {
        ExpectedResult::Value(value) => value,
        _ => return Err(IpiError::Failed),
    };
    // SAFETY: MIREG is reached only under the selector written above.
    let write = unsafe { swap_csr::<MIREG>(value) };
    // SAFETY: readback accesses that same selected MIREG.
    let readback = unsafe { probe_csr::<MIREG>() };
    // SAFETY: restore the selector read from this exact CSR.
    let restored = unsafe { swap_csr::<MISELECT>(original) };
    if matches!(write, ExpectedResult::Value(_))
        && matches!(readback, ExpectedResult::Value(actual) if actual == value)
        && matches!(restored, ExpectedResult::Value(actual) if actual == selector)
    {
        Ok(())
    } else {
        Err(IpiError::Failed)
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
