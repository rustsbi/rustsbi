//! Whole-machine shutdown and reboot capability.

use alloc::boxed::Box;
use alloc::sync::Arc;

mod arch;
mod device;
mod terminal;

pub(crate) use arch::halt;
pub(crate) use device::PowerDevice;
pub use terminal::abort;
pub(crate) use terminal::is_terminal;

/// Reason supplied to a whole-machine power action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerReason {
    /// No more specific reason is available.
    Unspecified,
    /// Firmware detected an unrecoverable system failure.
    SystemFailure,
}

/// Requested reboot style.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RebootKind {
    /// Reset all hardware state.
    Cold,
    /// Preserve any platform-defined warm-reset state.
    Warm,
}

/// Pre-commit power-control failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerError {
    /// The bound platform does not implement the requested action.
    Unsupported,
}

/// Authority to control whole-machine power state.
pub struct Power {
    device: Arc<dyn PowerDevice>,
}

impl Power {
    pub(crate) fn new(device: Box<dyn PowerDevice>) -> Option<Self> {
        let device = Arc::<dyn PowerDevice>::from(device);
        terminal::install_device(Arc::clone(&device)).then_some(Self { device })
    }

    /// Shuts down the complete machine.
    ///
    /// `Unsupported` is returned before any shared state changes. Soundness
    /// invariant: after terminal publication and peer notification, this call
    /// cannot return to a partially quiesced supervisor context.
    pub fn shutdown(&self, reason: PowerReason) -> PowerError {
        if !self.device.can_shutdown(reason) {
            return PowerError::Unsupported;
        }
        if !terminal::begin_power_transition() {
            arch::halt();
        }
        arch::mask_local_interrupts();
        crate::hart::notify_terminal_peers();
        // Security/compatibility policy: the validated explicit shutdown
        // request is committed only after peer quiescence becomes terminal.
        self.device.shutdown(reason)
    }

    /// Reboots the complete machine.
    ///
    /// `Unsupported` is returned before commit; every committed path diverges.
    pub fn reboot(&self, kind: RebootKind, reason: PowerReason) -> PowerError {
        if !self.device.can_reboot(kind, reason) {
            return PowerError::Unsupported;
        }
        if !terminal::begin_power_transition() {
            arch::halt();
        }
        arch::mask_local_interrupts();
        crate::hart::notify_terminal_peers();
        // Security/compatibility policy: reboot is selected only by this
        // explicit standard request, never as an automatic panic response.
        self.device.reboot(kind, reason)
    }
}
