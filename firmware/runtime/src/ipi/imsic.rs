//! Machine interrupt-file setup under Runtime's local IRQ exclusion.

use crate::{
    Error, Result,
    csr::{csrr, csrw},
    irq::LocalIrqDisabled,
};
use csr::aia::imsic::{Iid, ImsicRegister};
use csr::aia::machine::{Mireg, Miselect};

pub(super) fn initialize(num_ids: u16, iid: Iid, _irq: &LocalIrqDisabled) -> Result<()> {
    validate(num_ids, iid)?;
    // Runtime entry recovers absent indirect CSRs. Restore the selector even
    // when a selected interrupt-file register rejects an access.
    let previous = csrr!(Miselect)?;
    let result = (|| {
        write(ImsicRegister::Delivery, 0)?;
        write(ImsicRegister::Threshold, 0)?;
        // A selector covers 32 identities on RV32; RV64 uses every other
        // selector and covers 64 identities with each register.
        let selector_stride = (usize::BITS / 32) as usize;
        // Identity zero is reserved, but still occupies a register bit.
        let register_count = (usize::from(num_ids) + 1).div_ceil(32);
        for index in (0..register_count).step_by(selector_stride) {
            write(ImsicRegister::Pending(index as u8), 0)?;
            write(ImsicRegister::Enable(index as u8), 0)?;
        }
        let identity = usize::from(iid.number());
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

fn validate(num_ids: u16, iid: Iid) -> Result<()> {
    // IMSIC implements 64-bit groups including the reserved identity zero.
    if !(63..=2047).contains(&num_ids) || num_ids & 63 != 63 || iid.number() > num_ids {
        return Err(Error::InvalidArgs);
    }
    Ok(())
}

fn write(register: ImsicRegister, value: usize) -> Result<()> {
    // The parent owns the file and holds IRQ exclusion across selector/data.
    csrw!(Miselect, register.selector().ok_or(Error::InvalidArgs)?)?;
    csrw!(Mireg, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_identity_count_and_inclusive_upper_bound() {
        for count in [63, 127, 2047] {
            assert!(validate(count, Iid::new(count).unwrap()).is_ok());
        }
        assert!(validate(63, Iid::new(64).unwrap()).is_err());
        for count in [62, 64, 126, 2046, 2048] {
            assert!(validate(count, Iid::new(1).unwrap()).is_err());
        }
    }
}
