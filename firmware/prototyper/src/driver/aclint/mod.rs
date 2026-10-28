//! ACLINT machine-level timer and software-interrupt devices.

mod mswi;
mod mtimer;

use alloc::boxed::Box;
use runtime::{Result, ipi::IpiDevice, memory::MemoryRegistry, timer::TimerDevice};

use crate::platform::{AclintMswi, AclintMtimer};

pub(super) fn bind_timer(
    resource: &AclintMtimer,
    memory: &mut MemoryRegistry,
) -> Result<Box<dyn TimerDevice>> {
    Ok(Box::new(mtimer::bind(resource, memory)?))
}

pub(super) fn bind_ipi(
    resource: &AclintMswi,
    memory: &mut MemoryRegistry,
) -> Result<Box<dyn IpiDevice>> {
    Ok(Box::new(mswi::bind(resource, memory)?))
}
