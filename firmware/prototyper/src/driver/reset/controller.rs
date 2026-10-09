//! Ownership and synchronization for the selected reset device.

use alloc::boxed::Box;
use spin::Mutex;

use super::{ResetDevice, ResetError, ResetRequest};

/// A reset device with serialized command execution.
///
/// Concurrent requests cannot interleave device register transactions.
/// A successful reset does not return.
pub(crate) struct ResetController {
    device: Mutex<Box<dyn ResetDevice>>,
}

impl ResetController {
    pub(super) fn new(device: Box<dyn ResetDevice>) -> Self {
        Self {
            device: Mutex::new(device),
        }
    }

    /// Attempts the reset; a successful request does not return.
    pub(crate) fn reset(&self, request: ResetRequest) -> ResetError {
        self.device.lock().system_reset(request)
    }
}
