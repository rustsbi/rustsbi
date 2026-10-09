//! Permanent platform IPI ownership until the complete device set is published.

use alloc::boxed::Box;
use runtime::ipi::{InterruptSource, IpiDevice};
use spin::Once;

static DEVICE: Once<Box<dyn IpiDevice>> = Once::new();

pub(crate) fn init(device: Box<dyn IpiDevice>) -> &'static dyn IpiDevice {
    DEVICE.call_once(|| device).as_ref()
}

pub(crate) fn uses_imsic() -> bool {
    DEVICE
        .get()
        .is_some_and(|device| matches!(device.interrupt_source(), InterruptSource::Imsic(_)))
}
