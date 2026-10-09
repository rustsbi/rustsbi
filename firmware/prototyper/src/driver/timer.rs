//! Permanent platform timer ownership until the complete device set is published.

use alloc::boxed::Box;
use runtime::timer::TimerDevice;
use spin::Once;

static DEVICE: Once<Box<dyn TimerDevice>> = Once::new();

pub(crate) fn init(device: Box<dyn TimerDevice>) -> &'static dyn TimerDevice {
    DEVICE.call_once(|| device).as_ref()
}

pub(crate) fn get() -> Option<&'static dyn TimerDevice> {
    DEVICE.get().map(|device| device.as_ref())
}
