//! Whole-machine terminal arbitration and injected power control.

use alloc::boxed::Box;
use core::convert::Infallible;

mod arch;
mod control;
mod terminal;

pub(crate) use arch::halt;
pub use control::PowerControl;
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
    /// No provider is installed or it does not implement the requested action.
    Unsupported,
}

/// Failure while injecting the process-lifetime power provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerInstallError {
    /// A provider was already injected.
    AlreadyInstalled,
}

/// Injects the only whole-machine power-control implementation.
pub fn inject(control: Box<dyn PowerControl>) -> Result<(), PowerInstallError> {
    terminal::install_control(control)
}

/// Returns whether a power-control implementation has been injected.
pub fn available() -> bool {
    terminal::control().is_some()
}

/// Shuts down the complete machine.
///
/// Unsupported requests return before terminal publication. Once committed,
/// this function never returns, even if the provider's hardware operation
/// unexpectedly returns.
pub fn shutdown(reason: PowerReason) -> Result<Infallible, PowerError> {
    let control = terminal::control().ok_or(PowerError::Unsupported)?;
    if !control.can_shutdown(reason) {
        return Err(PowerError::Unsupported);
    }
    commit();
    control.shutdown(reason);
    arch::halt()
}

/// Reboots the complete machine.
///
/// Unsupported requests return before terminal publication; committed
/// requests never return.
pub fn reboot(kind: RebootKind, reason: PowerReason) -> Result<Infallible, PowerError> {
    let control = terminal::control().ok_or(PowerError::Unsupported)?;
    if !control.can_reboot(kind, reason) {
        return Err(PowerError::Unsupported);
    }
    commit();
    control.reboot(kind, reason);
    arch::halt()
}

fn commit() {
    if !terminal::begin_power_transition() {
        arch::halt();
    }
    arch::mask_local_interrupts();
    crate::hart::notify_terminal_peers();
}
