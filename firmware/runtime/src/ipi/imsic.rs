//! Machine interrupt-file setup under Runtime's local IRQ exclusion.

use crate::{
    Error, Result,
    csr::{csrr, csrw},
    irq::LocalIrqDisabled,
};
use csr::aia::imsic::{Iid, ImsicRegister};
use csr::aia::machine::{Mireg, Miselect};

/// Minimum interrupt identities implemented by an IMSIC file.
const MIN_IID_COUNT: u16 = 63;
/// Maximum interrupt identities implemented by an IMSIC file.
const MAX_IID_COUNT: u16 = Iid::MAX_NUMBER;
/// IMSIC files implement a count one less than a multiple of 64.
const IID_COUNT_GROUP: u16 = 64;
/// One IMSIC pending or enable register covers 32 interrupt identities.
const IID_PER_REGISTER: usize = 32;

pub(super) fn initialize(num_iids: u16, iid: Iid, _irq: &LocalIrqDisabled) -> Result<()> {
    validate_iid_capacity(num_iids, iid)?;
    // Runtime entry recovers absent indirect CSRs. Restore the selector even
    // when a selected interrupt-file register rejects an access.
    let previous = csrr!(Miselect)?;
    let result = (|| {
        write(ImsicRegister::Delivery, 0)?;
        write(ImsicRegister::Threshold, 0)?;
        // A selector covers 32 identities on RV32; RV64 uses every other
        // selector and covers 64 identities with each register.
        let selector_stride = usize::BITS as usize / IID_PER_REGISTER;
        // Identity zero is reserved, but still occupies a register bit.
        let register_count = (usize::from(num_iids) + 1).div_ceil(IID_PER_REGISTER);
        for index in (0..register_count).step_by(selector_stride) {
            write(ImsicRegister::Pending(index as u8), 0)?;
            write(ImsicRegister::Enable(index as u8), 0)?;
        }
        let identity = iid.number() as usize;
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

/// Validates an IMSIC file's identity capacity and the selected firmware identity.
///
/// AIA requires a capacity of 63 through 2047 that is one less than a multiple
/// of 64. See <https://docs.riscv.org/reference/aia/v1.0/IMSIC.html>.
pub fn validate_iid_capacity(num_iids: u16, iid: Iid) -> Result<()> {
    if !valid_iid_count(num_iids) || iid.number() > num_iids {
        return Err(Error::InvalidArgs);
    }
    Ok(())
}

fn valid_iid_count(num_iids: u16) -> bool {
    (MIN_IID_COUNT..=MAX_IID_COUNT).contains(&num_iids)
        && num_iids % IID_COUNT_GROUP == IID_COUNT_GROUP - 1
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
            assert!(validate_iid_capacity(count, Iid::new(count).unwrap()).is_ok());
        }
        assert!(validate_iid_capacity(63, Iid::new(64).unwrap()).is_err());
        for count in [62, 64, 126, 2046, 2048] {
            assert!(validate_iid_capacity(count, Iid::new(1).unwrap()).is_err());
        }
    }
}
