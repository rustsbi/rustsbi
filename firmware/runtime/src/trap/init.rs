//! One SBI policy publication per hart.
use super::Exception;
use crate::{Error, Result};
use alloc::boxed::Box;

use rustsbi::RustSBI;
use spin::Once;

use crate::hart::HartId;

/// Firmware policy for supervisor SBI calls and observed lower-mode exceptions.
pub trait TrapPolicy: RustSBI + Sync {
    /// Observes an exception before Runtime emulates or forwards it.
    fn on_exception(&self, _exception: Exception) {}

    /// Updates firmware policy before entering a committed non-retentive resume.
    fn on_nonretentive_resume(&self) {}
}

type Policy = &'static dyn TrapPolicy;
static POLICIES: Once<Box<[Once<Policy>]>> = Once::new();

fn policies() -> &'static [Once<Policy>] {
    POLICIES.call_once(|| HartId::all().map(|_| Once::new()).collect())
}

pub(super) fn policy(hart: HartId) -> Policy {
    *policies()[hart.index()]
        .get()
        .expect("SBI ecall before trap::init published its policy")
}

/// Publishes the current hart's SBI policy once.
///
/// Runtime startup installs the exception entry before policy initialization.
pub fn init<P: TrapPolicy + 'static>(policy: &'static P) -> Result<()> {
    let hart = HartId::current()?;
    let mut published = false;
    policies()[hart.index()].call_once(|| {
        published = true;
        policy as Policy
    });
    if published {
        Ok(())
    } else {
        Err(Error::AlreadyInitialized)
    }
}
