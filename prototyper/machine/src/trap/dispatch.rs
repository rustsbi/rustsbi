//! Validation and closed routing of stack-resident trap frames.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

use super::frame::Frame;
use super::{Cause, SbiHandler, Trap, TrapEvent, arch, stack};

const HANDLER_EMPTY: u8 = 0;
const HANDLER_WRITING: u8 = 1;
const HANDLER_READY: u8 = 2;

struct InstalledHandler {
    state: AtomicU8,
    handler: UnsafeCell<Option<&'static dyn SbiHandler>>,
}

// SAFETY: boot initializes the reference once before Release publication.
// Dispatch first observes HANDLER_READY with Acquire and the referent is
// intentionally leaked for the remaining firmware lifetime.
unsafe impl Sync for InstalledHandler {}

static INSTALLED_HANDLER: InstalledHandler = InstalledHandler {
    state: AtomicU8::new(HANDLER_EMPTY),
    handler: UnsafeCell::new(None),
};

pub(crate) fn install(handler: &'static dyn SbiHandler) -> Result<(), ()> {
    INSTALLED_HANDLER
        .state
        .compare_exchange(
            HANDLER_EMPTY,
            HANDLER_WRITING,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .map_err(|_| ())?;
    // SAFETY: this caller uniquely owns HANDLER_WRITING.
    unsafe { INSTALLED_HANDLER.handler.get().write(Some(handler)) };
    INSTALLED_HANDLER
        .state
        .store(HANDLER_READY, Ordering::Release);
    Ok(())
}

fn handler() -> Option<&'static dyn SbiHandler> {
    if INSTALLED_HANDLER.state.load(Ordering::Acquire) != HANDLER_READY {
        return None;
    }
    // SAFETY: Acquire observed the sole Release publication.
    unsafe { *INSTALLED_HANDLER.handler.get() }
}

/// Enters Rust after assembly has published one complete trap frame.
///
/// # Safety
///
/// `frame` must equal the primary frame address derived from `stack_top`.
/// Entry must have initialized every field, kept MIE clear, restored
/// `mscratch` to `stack_top`, and diverted every nested trap to the bounded
/// assembly fatal path before this call.
pub(super) unsafe extern "C" fn dispatch(frame: *mut Frame, stack_top: usize) -> ! {
    // Soundness invariant: a peer's Release terminal publication is observed
    // before any interrupt or exception can re-enter upper trap policy.
    if crate::power::is_terminal() {
        crate::power::halt();
    }
    let address = frame as usize;
    if stack::primary_frame(stack_top) != Some(address) {
        crate::power::abort(|| {});
    }

    // SAFETY: the exact fixed primary-frame relation was validated above.
    // Assembly initialized the complete frame and no nested path reaches Rust.
    let frame = unsafe { &mut *frame };
    let Some(index) = stack::index_for_top(stack_top) else {
        crate::power::abort(|| {});
    };
    if super::features::hypervisor_metadata_available(index) {
        arch::capture_hypervisor_metadata(frame);
    }
    let cause = frame.cause();
    let machine_origin = frame.previous_mode() == 3;
    let notification = match cause {
        Cause::MachineSoftwareInterrupt => Some(crate::hart::Notification::Software),
        Cause::MachineExternalInterrupt => Some(crate::hart::Notification::External),
        _ => None,
    };
    if let Some(notification) = notification {
        let Some(runtime) = crate::hart::runtime::runtime() else {
            crate::power::abort(|| {});
        };
        if !runtime.handles_notification(notification) || runtime.handle_notification().is_err() {
            crate::power::abort(|| {});
        }
        arch::restore(Trap { frame, stack_top })
    }
    // The only admitted first-level M-origin trap is a machine notification
    // used to wake the private warm loop. Exceptions or unrelated interrupts
    // in machine code are lower-runtime failures and must never reach the
    // safe upper handler as though they came from the next stage.
    if machine_origin {
        crate::power::abort(|| {})
    }
    if matches!(cause, Cause::MachineTimerInterrupt) && crate::timer::handle_interrupt() {
        arch::restore(Trap { frame, stack_top })
    }
    let Some(handler) = handler() else {
        crate::power::abort(|| {});
    };
    let trap = Trap { frame, stack_top };
    match cause {
        Cause::SbiCall(call) => {
            let response = handler.handle_ecall(call);
            trap.resume_from_ecall(response.error, response.value)
        }
        Cause::IllegalInstruction => {
            handler.observe_trap(TrapEvent::IllegalInstruction);
            trap.emulate_illegal()
        }
        Cause::LoadMisaligned => {
            handler.observe_trap(TrapEvent::MisalignedLoad);
            trap.redirect()
        }
        Cause::StoreMisaligned => {
            handler.observe_trap(TrapEvent::MisalignedStore);
            trap.redirect()
        }
        Cause::Other => trap.redirect(),
        Cause::MachineSoftwareInterrupt
        | Cause::MachineTimerInterrupt
        | Cause::MachineExternalInterrupt => crate::power::abort(|| {}),
    }
}
