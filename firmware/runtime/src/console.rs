//! Platform console registration and byte-oriented device access.

use alloc::boxed::Box;
use spin::{Mutex, MutexGuard, Once};

use crate::{Error, Result};

/// A non-blocking, byte-oriented console device.
///
/// Each operation transfers at most one contiguous slice.
/// `Ok(0)` means no progress.
pub trait ConsoleDevice: Send {
    /// Writes up to `src.len()` bytes, returning the number accepted.
    ///
    /// Returns [`Error::AccessDenied`] if access is denied or [`Error::IoError`]
    /// if the device fails.
    fn write_slice(&mut self, src: &[u8]) -> Result<usize>;

    /// Reads up to `dst.len()` bytes, returning the number received.
    ///
    /// Returns [`Error::AccessDenied`] if access is denied or [`Error::IoError`]
    /// if the device fails.
    fn read_slice(&mut self, dst: &mut [u8]) -> Result<usize>;
}

static CONSOLE: Once<Mutex<Box<dyn ConsoleDevice>>> = Once::new();

/// Registers the platform's initialized console device.
///
/// A second registration returns [`Error::AlreadyInitialized`].
pub fn init(device: Box<dyn ConsoleDevice>) -> Result<()> {
    let mut initialized = false;
    CONSOLE.call_once(|| {
        initialized = true;
        Mutex::new(device)
    });
    initialized.then_some(()).ok_or(Error::AlreadyInitialized)
}

/// Locks the registered console device, or returns `None` if none was registered.
pub fn get() -> Option<MutexGuard<'static, Box<dyn ConsoleDevice>>> {
    CONSOLE.get().map(Mutex::lock)
}
