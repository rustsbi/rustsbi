//! Timer device class.

mod andes;
mod clint;
mod plmt;

use alloc::boxed::Box;
use runtime::{Error, Result, hart::HartId, timer::TimerDevice};

use crate::driver::device::{self, Class, Drivers};

pub(crate) use crate::driver::device::Driver;

/// The timer class.
pub(crate) struct Timer;

impl Class for Timer {
    type Device = Box<dyn TimerDevice>;
}

pub(crate) type Description = device::Description<Timer>;

pub(crate) static DRIVERS: Drivers<Timer> =
    Drivers::new(&[&andes::PLMT, &clint::SIFIVE, &clint::THEAD]);

pub(super) fn hart_id_upper_bound() -> Result<usize> {
    HartId::all()
        .last()
        .and_then(|hart| hart.as_usize().checked_add(1))
        .ok_or(Error::InvalidArgs)
}
