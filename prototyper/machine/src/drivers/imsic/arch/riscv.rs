//! RISC-V IMSIC CSR and ordering operations.

use super::super::ImsicError;
use crate::trap::expected::{ExpectedResult, probe_csr, swap_csr};

const MISELECT: u16 = 0x350;
const MIREG: u16 = 0x351;
const MTOPEI: u16 = 0x35c;
const EIDELIVERY: usize = 0x70;
const EITHRESHOLD: usize = 0x72;
const EIP_BASE: usize = 0x80;
const EIE_BASE: usize = 0xc0;

pub(in crate::drivers::imsic) fn initialize_current_file(
    num_ids: u16,
    firmware_iid: u16,
) -> Result<(), ImsicError> {
    // SAFETY: selectors are closed constants derived from the validated count;
    // delivery remains disabled until all writes and readbacks succeed.
    unsafe {
        indirect_write(EIDELIVERY, 0)?;
        indirect_write(EITHRESHOLD, 0)?;
        let word_bits = usize::BITS as usize;
        let word_count = usize::from(num_ids).div_ceil(word_bits);
        let selector_stride = if usize::BITS == 64 { 2 } else { 1 };
        for word in 0..word_count {
            indirect_write(EIP_BASE + word * selector_stride, 0)?;
            indirect_write(EIE_BASE + word * selector_stride, 0)?;
        }
        let iid = usize::from(firmware_iid);
        indirect_write(
            EIE_BASE + (iid / word_bits) * selector_stride,
            1usize << (iid % word_bits),
        )?;
        indirect_write(EIDELIVERY, 1)?;
    }
    Ok(())
}

unsafe fn indirect_write(selector: usize, value: usize) -> Result<(), ImsicError> {
    // SAFETY: fixed IMSIC CSR and a validated closed selector.
    let original_select = match unsafe { swap_csr::<MISELECT>(selector) } {
        ExpectedResult::Value(value) => value,
        _ => return Err(ImsicError::Hardware),
    };
    // SAFETY: MIREG is accessed only under the selected closed register.
    let write = unsafe { swap_csr::<MIREG>(value) };
    // SAFETY: readback of the same selected register.
    let readback = unsafe { probe_csr::<MIREG>() };
    // SAFETY: restores the exact selector captured above.
    let restored = unsafe { swap_csr::<MISELECT>(original_select) };
    if matches!(write, ExpectedResult::Value(_))
        && matches!(readback, ExpectedResult::Value(actual) if actual == value)
        && matches!(restored, ExpectedResult::Value(actual) if actual == selector)
    {
        Ok(())
    } else {
        Err(ImsicError::Hardware)
    }
}

pub(in crate::drivers::imsic) fn current_hart_id() -> usize {
    let value;
    // SAFETY: `mhartid` is a mandatory read-only machine CSR.
    unsafe {
        core::arch::asm!("csrr {value}, mhartid", value = out(reg) value, options(nomem, nostack))
    };
    value
}

pub(in crate::drivers::imsic) fn claim_current_file(hart_id: usize) {
    if hart_id != current_hart_id() {
        return;
    }
    // SAFETY: preparation enabled only the firmware IID in this file.
    let _ = unsafe { swap_csr::<MTOPEI>(0) };
    device_fence();
}

pub(in crate::drivers::imsic) fn device_fence() {
    // SAFETY: the full device fence carries no memory operand.
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) }
}
