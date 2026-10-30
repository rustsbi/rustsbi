//! Machine interrupt-file setup under Runtime's local IRQ exclusion.

use crate::{
    Error, Result,
    fault_recovery::{csrr, csrw},
    irq::LocalIrqDisabled,
};
use core::num::NonZeroU16;
use csr::aia::imsic::ImsicRegister;

/// A 12-bit IMSIC interrupt identity used by firmware IPIs.
///
/// Zero is not an identity. The interrupt file's `num_ids` bounds the
/// identities a platform may use; discovery validates that bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Iid(NonZeroU16);

impl Iid {
    /// Constructs an identity from its 12-bit encoding.
    #[inline]
    pub const fn new(number: u16) -> Option<Self> {
        if number == 0 || number > 0x7ff {
            return None;
        }
        match NonZeroU16::new(number) {
            Some(number) => Some(Self(number)),
            None => None,
        }
    }

    /// Returns the 12-bit identity encoding.
    #[inline]
    pub const fn number(self) -> u16 {
        self.0.get()
    }
}
use csr::aia::machine::{Mireg, Miselect};

pub(super) fn initialize(num_ids: u16, iid: Iid, _irq: &LocalIrqDisabled) -> Result<()> {
    if !(63..=2047).contains(&num_ids) || iid.number() >= num_ids {
        return Err(Error::InvalidArgs);
    }
    // Runtime entry recovers absent indirect CSRs. Restore the selector even
    // when a selected interrupt-file register rejects an access.
    let previous = csrr!(Miselect)?;
    let result = configure_interrupt_file(num_ids, iid);
    csrw!(Miselect, previous)?;
    result
}

fn configure_interrupt_file(num_ids: u16, iid: Iid) -> Result<()> {
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
    let identity = usize::from(iid.number());
    let word = identity / usize::BITS as usize;
    write(
        ImsicRegister::Enable((word * selector_stride) as u8),
        1usize << (identity % usize::BITS as usize),
    )?;
    write(ImsicRegister::Delivery, 1)
}

fn write(register: ImsicRegister, value: usize) -> Result<()> {
    // The parent owns the file and holds IRQ exclusion across selector/data.
    let selector = register.selector().ok_or(Error::InvalidArgs)?;
    csrw!(Miselect, selector)?;
    csrw!(Mireg, value)
}
