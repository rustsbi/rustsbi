//! Platform reset-device ownership and reset requests.

use alloc::boxed::Box;
use core::convert::Infallible;
use spin::{Mutex, Once};

use crate::{Error, Result};

/// A supported system-reset type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetType {
    /// A request to power off the system.
    Shutdown,
    /// A request to perform a cold reboot.
    ColdReboot,
    /// A request to perform a warm reboot.
    WarmReboot,
    /// A vendor- or platform-specific reset type.
    VendorSpecific(u32),
}

/// The reason supplied with a system-reset request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetReason {
    /// No specific reason.
    NoReason,
    /// A system failure.
    SystemFailure,
    /// An SBI implementation-specific reason.
    SbiSpecific(u32),
    /// A vendor- or platform-specific reason.
    VendorSpecific(u32),
}

/// A validated system-reset request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetRequest {
    reset_type: ResetType,
    reset_reason: ResetReason,
}

impl ResetRequest {
    /// Creates a request from a reset type and reason.
    pub const fn new(reset_type: ResetType, reset_reason: ResetReason) -> Self {
        Self {
            reset_type,
            reset_reason,
        }
    }

    /// Returns the requested reset type.
    pub const fn reset_type(self) -> ResetType {
        self.reset_type
    }

    /// Returns the supplied reset reason.
    pub const fn reset_reason(self) -> ResetReason {
        self.reset_reason
    }
}

/// A platform system-reset device.
pub trait ResetDevice: Send {
    /// Attempts the requested reset.
    ///
    /// Returns [`Error::InvalidArgs`] if this device cannot represent the
    /// request, or [`Error::IoError`] if the reset operation fails.
    /// A successful reset does not return, so `Ok` is uninhabited.
    fn system_reset(&mut self, request: ResetRequest) -> Result<Infallible>;
}

/// The registered reset device, shared by all harts.
pub struct Reset {
    device: Mutex<Box<dyn ResetDevice>>,
}

impl Reset {
    /// Attempts a reset while excluding concurrent reset requests.
    ///
    /// A successful reset does not return, so `Ok` is uninhabited.
    pub fn reset(&self, request: ResetRequest) -> Result<Infallible> {
        self.device.lock().system_reset(request)
    }
}

static RESET: Once<Reset> = Once::new();

/// Registers the initialized platform reset device.
///
/// A second registration returns [`Error::AlreadyInitialized`].
pub fn init(device: Box<dyn ResetDevice>) -> Result<()> {
    let mut initialized = false;
    RESET.call_once(|| {
        initialized = true;
        Reset {
            device: Mutex::new(device),
        }
    });
    initialized.then_some(()).ok_or(Error::AlreadyInitialized)
}

/// Returns the registered reset device, if present.
pub fn get() -> Option<&'static Reset> {
    RESET.get()
}
