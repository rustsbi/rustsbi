//! IPI device class.

mod aclint;
mod allwinner;
mod plicsw;

use alloc::boxed::Box;
use runtime::{Result, ipi::IpiDevice};

use crate::driver::device::{self, Class, Drivers};

pub(crate) use crate::driver::device::Driver;
use crate::driver::timer::hart_id_upper_bound;

/// The IPI class.
pub(crate) struct Ipi;

impl Class for Ipi {
    type Device = Box<dyn IpiDevice>;
}

pub(crate) type Description = device::Description<Ipi>;

pub(crate) static DRIVERS: Drivers<Ipi> = Drivers::new(&[&allwinner::PLIC_SW, &aclint::MSWI]);

pub(super) fn hart_bound() -> Result<usize> {
    hart_id_upper_bound()
}
