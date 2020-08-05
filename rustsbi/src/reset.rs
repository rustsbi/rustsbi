//! Legacy reset method

/// Legacy reset method
pub trait Reset: Send {
    /// Puts all the harts to shut down state from supervisor point of view. This SBI call doesn’t return.
    fn reset(&self) -> !;
}

use alloc::boxed::Box;
use spin::Mutex;

lazy_static::lazy_static! {
    static ref RESET: Mutex<Option<Box<dyn Reset>>> =
        Mutex::new(None);
}

#[doc(hidden)] // use through a macro
pub fn init_reset<T: Reset + Send + 'static>(reset: T) {
    *RESET.lock() = Some(Box::new(reset));
}

pub(crate) fn reset() -> ! {
    if let Some(obj) = &*RESET.lock() {
        obj.reset();
    }
    panic!("no reset handler available")
}
