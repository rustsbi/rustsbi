//! Machine IMSIC interrupt-file operations under local IRQ exclusion.

use crate::{Error, Result, boot, hart::Detect, irq::LocalIrqDisabled};
use csr::aia::imsic::{Iid, ImsicRegister};
use csr::aia::machine::{Mireg, Miselect, Mtopei};
use csr::machine;

struct Imsic {
    iid: Iid,
    num_iids: u16,
}

/// Validates the identity capacity and the selected firmware identity.
pub fn validate_iid_capacity(num_iids: u16, iid: Iid) -> Result<()> {
    ((MIN_IID_COUNT..=MAX_IID_COUNT).contains(&num_iids)
        && num_iids % IID_COUNT_GROUP == IID_COUNT_GROUP - 1
        && iid.number() <= num_iids)
        .then_some(())
        .ok_or(Error::InvalidArgs)
}

pub(super) fn initialize(num_iids: u16, iid: Iid, irq: &LocalIrqDisabled) -> Result<()> {
    validate_iid_capacity(num_iids, iid)?;
    Imsic { iid, num_iids }.init_hart(irq)
}

const MIN_IID_COUNT: u16 = 63;
const MAX_IID_COUNT: u16 = Iid::MAX_NUMBER;
/// IMSIC files implement a count one less than a multiple of 64.
const IID_COUNT_GROUP: u16 = 64;
const IID_PER_REGISTER: usize = 32;

impl Imsic {
    fn detect(_irq: &LocalIrqDisabled) -> Result<()> {
        if !boot::traps_ready() || usize::from(machine::Scratch::read()) != 0 {
            return Err(Error::NotReady);
        }
        // SAFETY:
        // 1. Runtime trap recovery is installed, and `MPRV` and `mscratch` are clear.
        // 2. The IRQ context keeps the probe on the current hart without reentry.
        unsafe { Mtopei::detect() }.map_err(|_| Error::NotSupported)
    }

    pub(super) fn init_hart(&self, irq: &LocalIrqDisabled) -> Result<()> {
        Self::detect(irq)?;
        // RV32 uses each selector; RV64 uses every other selector.
        let selector_stride = usize::BITS as usize / IID_PER_REGISTER;
        // Identity zero is reserved, but still occupies a register bit.
        let register_count = (usize::from(self.num_iids) + 1).div_ceil(IID_PER_REGISTER);
        let identity = self.iid.number() as usize;
        let word = identity / usize::BITS as usize;

        for register in [ImsicRegister::Delivery, ImsicRegister::Threshold] {
            let selector = register.selector().ok_or(Error::InvalidArgs)?;
            // SAFETY:
            // 1. Runtime owns this hart's IMSIC file; `selector` and zero are
            //    valid for configuring it.
            // 2. The IRQ context serializes the selector and data accesses.
            unsafe {
                Miselect::write(selector.into());
                Mireg::write(0);
            }
        }
        for index in (0..register_count).step_by(selector_stride) {
            for register in [
                ImsicRegister::Pending(index as u8),
                ImsicRegister::Enable(index as u8),
            ] {
                let selector = register.selector().ok_or(Error::InvalidArgs)?;
                // SAFETY:
                // 1. Runtime owns this hart's IMSIC file; `selector` and zero
                //    are valid for configuring it.
                // 2. The IRQ context serializes indirect accesses.
                unsafe {
                    Miselect::write(selector.into());
                    Mireg::write(0);
                }
            }
        }
        for (register, value) in [
            (
                ImsicRegister::Enable((word * selector_stride) as u8),
                1usize << (identity % usize::BITS as usize),
            ),
            (ImsicRegister::Delivery, 1),
        ] {
            let selector = register.selector().ok_or(Error::InvalidArgs)?;
            // SAFETY:
            // 1. Runtime owns this hart's IMSIC file; `selector` and `value`
            //    are valid for configuring the firmware IPI.
            // 2. The IRQ context serializes indirect accesses.
            unsafe {
                Miselect::write(selector.into());
                Mireg::write(value);
            }
        }
        Ok(())
    }
}
