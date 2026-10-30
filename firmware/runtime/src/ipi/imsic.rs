//! Machine interrupt-file setup under Runtime's local IRQ exclusion.

use crate::{
    Error, Result,
    csr::{csrr, csrw},
    irq::LocalIrqDisabled,
};
use core::num::NonZeroU16;
use csr::aia::imsic::ImsicRegister;
use csr::aia::machine::{Mireg, Miselect};

pub(super) fn initialize(num_ids: u16, iid: NonZeroU16, _irq: &LocalIrqDisabled) -> Result<()> {
    if !(63..=2047).contains(&num_ids) || iid.get() >= num_ids {
        return Err(Error::InvalidArgs);
    }
    // Runtime entry recovers absent indirect CSRs. Restore the selector even
    // when a selected interrupt-file register rejects an access.
    let previous = csrr!(Miselect)?;
    let result = (|| {
        write(ImsicRegister::Delivery, 0)?;
        write(ImsicRegister::Threshold, 0)?;
        // A selector covers 32 identities on RV32; RV64 uses every other
        // selector and covers 64 identities with each register.
        let selector_stride = (usize::BITS / 32) as usize;
        let register_count = usize::from(num_ids).div_ceil(32);
        for index in (0..register_count).step_by(selector_stride) {
            write(ImsicRegister::Pending(index as u8), 0)?;
            write(ImsicRegister::Enable(index as u8), 0)?;
        }
        let identity = usize::from(iid.get());
        let word = identity / usize::BITS as usize;
        write(
            ImsicRegister::Enable((word * selector_stride) as u8),
            1usize << (identity % usize::BITS as usize),
        )?;
        write(ImsicRegister::Delivery, 1)
    })();
    csrw!(Miselect, previous)?;
    result
}

fn write(register: ImsicRegister, value: usize) -> Result<()> {
    // The parent owns the file and holds IRQ exclusion across selector/data.
    csrw!(Miselect, register.selector().ok_or(Error::InvalidArgs)?)?;
    csrw!(Mireg, value)
}
