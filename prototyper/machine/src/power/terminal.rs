//! Always-available global failure transition.

use alloc::boxed::Box;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use spin::Once;

use super::{PowerControl, PowerReason};

#[derive(Clone, Copy)]
#[repr(u8)]
enum Phase {
    Running,
    Aborting,
    PowerTransition,
}

impl Phase {
    const fn raw(self) -> u8 {
        self as u8
    }
}

static STATE: AbortState = AbortState::new();
static CONTROL: Once<Box<dyn PowerControl>> = Once::new();
static CONTROL_CLAIMED: AtomicBool = AtomicBool::new(false);

struct AbortState {
    phase: AtomicU8,
}

impl AbortState {
    const fn new() -> Self {
        Self {
            phase: AtomicU8::new(Phase::Running.raw()),
        }
    }

    fn begin(&self) -> bool {
        // Soundness invariant: the Release half publishes fatal state before
        // reporting or device access; every machine re-entry boundary uses an
        // Acquire observation and refuses to resume abandoned upper state.
        self.phase
            .compare_exchange(
                Phase::Running.raw(),
                Phase::Aborting.raw(),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn begin_power(&self) -> bool {
        self.phase
            .compare_exchange(
                Phase::Running.raw(),
                Phase::PowerTransition.raw(),
                Ordering::AcqRel,
                Ordering::Acquire,
            )
            .is_ok()
    }

    fn terminal(&self) -> bool {
        self.phase.load(Ordering::Acquire) != Phase::Running.raw()
    }
}

pub(crate) fn install_control(control: Box<dyn PowerControl>) -> Option<()> {
    claim_control(&CONTROL_CLAIMED).then_some(())?;
    CONTROL.call_once(|| control);
    Some(())
}

fn claim_control(claimed: &AtomicBool) -> bool {
    claimed
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

pub(crate) fn control() -> Option<&'static dyn PowerControl> {
    CONTROL.get().map(Box::as_ref)
}

pub(crate) fn begin_power_transition() -> bool {
    STATE.begin_power()
}

pub(crate) fn is_terminal() -> bool {
    STATE.terminal()
}

/// Transitions the complete firmware into its terminal failure state.
///
/// Exactly one caller runs `report`. Recursive or concurrent callers skip it,
/// so panic reporting cannot wait for a console owner that may never release.
///
/// Soundness invariant: an arbitrary panic can abandon a shared lock or only
/// part of a Rust transition. Fatal state is therefore published before the
/// callback, and no hart that observes it may return to upper or lower-mode
/// code. Abandoned locks are never force-unlocked, and the callback must not
/// retain `PanicInfo`, `fmt::Arguments`, or any other borrowed report value.
pub fn abort(report: impl FnOnce()) -> ! {
    if STATE.begin() {
        super::mask_local_interrupts();
        crate::hart::notify_terminal_peers();
        report();
        // Security/compatibility policy: a fatal firmware failure requests
        // shutdown, never automatic reboot. Missing or unsupported control
        // falls through to the same terminal wait.
        if let Some(control) = CONTROL.get()
            && control.can_shutdown(PowerReason::SystemFailure)
        {
            control.shutdown(PowerReason::SystemFailure);
        }
    }
    super::halt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_caller_wins_the_report_transition() {
        let state = AbortState::new();
        assert!(state.begin());
        assert!(!state.begin());
    }

    #[test]
    fn power_and_abort_are_mutually_terminal() {
        let power = AbortState::new();
        assert!(power.begin_power());
        assert!(!power.begin());
        assert!(power.terminal());

        let abort = AbortState::new();
        assert!(abort.begin());
        assert!(!abort.begin_power());
        assert!(abort.terminal());
    }

    #[test]
    fn power_provider_can_be_claimed_exactly_once() {
        let claimed = AtomicBool::new(false);
        assert!(claim_control(&claimed));
        assert!(!claim_control(&claimed));
    }
}
