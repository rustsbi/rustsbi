//! Ownership and synchronization for the selected reset backend.

use alloc::boxed::Box;
use spin::Mutex;

use super::{ResetBackend, ResetError, ResetRequest};

/// Owns the selected reset backend and serializes reset commands.
///
/// SRST may be entered concurrently by multiple harts. The mutex ensures that
/// multi-register watchdog and PMIC transactions cannot interleave; a
/// successful request never releases it because the machine resets first.
#[derive(Default)]
pub(crate) struct ResetDevice {
    backend: Option<Mutex<Box<dyn ResetBackend>>>,
}

impl ResetDevice {
    pub(super) fn new(backend: Option<Box<dyn ResetBackend>>) -> Self {
        Self {
            backend: backend.map(Mutex::new),
        }
    }

    pub(crate) fn is_available(&self) -> bool {
        self.backend.is_some()
    }

    /// A successful reset does not return.
    pub(crate) fn reset(&self, request: ResetRequest) -> Option<ResetError> {
        let backend = self.backend.as_ref()?;
        let mut backend = backend.lock();
        Some(backend.system_reset(request))
    }
}
