//! Whole-machine terminal arbitration and injected power control.

use alloc::boxed::Box;
use core::convert::Infallible;

mod terminal;

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

/// A bound platform device that can terminate or reset the whole machine.
///
/// The selected device owns its MMIO convention.  The caller checks support
/// before publishing the irreversible terminal transition, so a supported
/// action must not later return from its matching commit method.
pub trait PowerControl: Send + Sync + 'static {
    /// Reports whether a shutdown may commit this reason.
    fn can_shutdown(&self, reason: PowerReason) -> bool;

    /// Reports whether a reboot may commit this kind and reason.
    fn can_reboot(&self, kind: RebootKind, reason: PowerReason) -> bool;

    /// Commits the previously accepted shutdown.
    fn shutdown(&self, reason: PowerReason);

    /// Commits the previously accepted reboot.
    fn reboot(&self, kind: RebootKind, reason: PowerReason);
}

/// Pre-commit power-control failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PowerError {
    /// No provider is installed or it does not implement the requested action.
    Unsupported,
}

/// Injects the only whole-machine power-control implementation.
pub fn inject(control: Box<dyn PowerControl>) -> Option<()> {
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
    halt()
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
    halt()
}

fn commit() {
    if !terminal::begin_power_transition() {
        halt();
    }
    mask_local_interrupts();
    crate::hart::notify_terminal_peers();
}

fn mask_local_interrupts() {
    // SAFETY: terminal transition changes only local `mie` and never resumes.
    unsafe { core::arch::asm!("csrw mie, zero", options(nostack)) }
}

pub(crate) fn halt() -> ! {
    // Architecture invariant: every local source is masked before waiting.
    unsafe {
        core::arch::asm!("csrw mie, zero", options(nostack));
        loop {
            core::arch::asm!("wfi", options(nomem, nostack));
        }
    }
}
