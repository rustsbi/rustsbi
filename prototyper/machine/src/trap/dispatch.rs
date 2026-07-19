//! Validation and dispatch of frames published by trap-entry assembly.

use core::sync::atomic::{AtomicUsize, Ordering};

use super::Trap;
use super::entry::{HartTrapState, restore};
use super::frame::{self, Frame};

static FATAL_CAUSE: AtomicUsize = AtomicUsize::new(usize::MAX);
static FATAL_PC: AtomicUsize = AtomicUsize::new(0);

/// Enters Rust after assembly has published one complete trap frame.
///
/// # Safety
///
/// `frame` must be aligned, fully initialized, uniquely owned storage within
/// the current hart's configured trap stack. `state` must be that hart's
/// initialized `mscratch` state, and its published current-frame address and
/// depth must match `frame` and `depth`. Entry must keep MIE clear, establish
/// M-mode stack semantics with MPRV clear, and publish all frame fields before
/// this call. A depth-one call transfers unique frame authority to this
/// function; a nested call must use disjoint complete storage and can only
/// enter the bounded fatal branch.
pub(super) unsafe extern "C" fn dispatch(frame: *mut Frame, state: *const (), depth: usize) -> ! {
    // Soundness invariant: a peer's Release terminal publication is observed
    // before any interrupt or exception can re-enter upper trap policy.
    if crate::power::is_terminal() {
        crate::power::halt();
    }
    let state = state.cast::<HartTrapState>();
    if depth != 1 {
        // SAFETY: assembly completed this disjoint nested frame before calling
        // Rust. The volatile reads retain bounded machine-only diagnostics and
        // do not construct a reference to the live outer frame.
        let cause = unsafe {
            frame
                .cast::<u8>()
                .add(frame::CAUSE_OFFSET)
                .cast::<usize>()
                .read()
        };
        // SAFETY: same complete-frame argument; MEPC is an initialized field.
        let pc = unsafe {
            frame
                .cast::<u8>()
                .add(frame::MEPC_OFFSET)
                .cast::<usize>()
                .read()
        };
        FATAL_CAUSE.store(cause, Ordering::Release);
        FATAL_PC.store(pc, Ordering::Release);
        // TODO: Add a machine-only recovery branch only after proving its exact
        // faulting instruction and phase, unique continuation, preservation of
        // live Rust borrows and machine state, and inability to enter the upper
        // TrapHandler. A cause value alone is not sufficient evidence for
        // recovery.
        crate::power::abort(|| {})
    }

    // Validate the assembly-to-Rust relation before forming either reference.
    // SAFETY: the raw pointer is not dereferenced until its frame relation has
    // been checked against the state installed in mscratch.
    let (bottom, top, current, published_depth) = unsafe { (&*state).frame_publication() };
    let address = frame as usize;
    let Some(frame_end) = address.checked_add(frame::FRAME_SIZE) else {
        crate::power::abort(|| {});
    };
    if current != address
        || published_depth != 1
        || !address.is_multiple_of(frame::FRAME_ALIGN)
        || address < bottom
        || frame_end > top
    {
        crate::power::abort(|| {});
    }

    // SAFETY: the checks above repeat entry's stack bounds/alignment and exact
    // current-frame publication. Assembly initialized every field, admitted
    // depth one, and no other Rust reference can name this frame.
    let (frame, state) = unsafe { (&mut *frame, &*state) };
    let cause = frame.cause();
    let machine_origin = frame.previous_mode() == 3;
    let notification = match cause {
        super::Cause::MachineSoftwareInterrupt => Some(crate::hart::Notification::Software),
        super::Cause::MachineExternalInterrupt => Some(crate::hart::Notification::External),
        _ => None,
    };
    if let Some(notification) = notification {
        let Some(runtime) = crate::hart::runtime::runtime() else {
            crate::power::abort(|| {});
        };
        if !runtime.handles_notification(notification) || runtime.handle_notification().is_err() {
            crate::power::abort(|| {});
        }
        restore(Trap { frame, state })
    }
    // The only admitted first-level M-origin trap is a machine notification
    // used to wake the private warm loop. Exceptions or unrelated interrupts
    // in machine code are lower-runtime failures and must never reach the
    // safe upper handler as though they came from the next stage.
    if machine_origin {
        crate::power::abort(|| {})
    }
    if matches!(cause, super::Cause::MachineTimerInterrupt) && state.handle_timer_interrupt() {
        restore(Trap { frame, state })
    }
    let Some(handler) = state.handler() else {
        crate::power::abort(|| {});
    };
    handler.handle(Trap { frame, state })
}
