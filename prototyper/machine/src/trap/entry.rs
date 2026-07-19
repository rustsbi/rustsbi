//! Private trap-stack ownership and direct machine trap entry.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use alloc::sync::Arc;

use super::TrapHandler;
use super::expected::ExpectedTrapRecord;
use super::frame;
use crate::PerformanceCounters;
use crate::config::{HART_CAPACITY, TRAP_STACK_SIZE};
use crate::timer::{TimerDevice, TimerError};

mod arch;

pub(crate) use arch::{
    activate, current_index, enter_resumed_stage, park_current_hart, prepare_counters,
    prepare_hypervisor_metadata,
};
pub(super) use arch::{current_state, restore};

const TRAP_STATE_EMPTY: u32 = 0;
const TRAP_STATE_WRITING: u32 = 1;
const TRAP_STATE_READY: u32 = 2;
const FLAG_HYPERVISOR: usize = 1;

#[repr(C, align(16))]
struct TrapStack(UnsafeCell<[MaybeUninit<u8>; TRAP_STACK_SIZE]>);

// SAFETY: a stack is assigned to exactly one admitted dense hart position and
// is never exposed as a Rust slice. Entry assembly alone creates disjoint frame
// and call-stack regions within its fixed bounds.
unsafe impl Sync for TrapStack {}

impl TrapStack {
    const fn new() -> Self {
        Self(UnsafeCell::new([MaybeUninit::uninit(); TRAP_STACK_SIZE]))
    }

    fn bounds(&self) -> (usize, usize) {
        let bottom = self.0.get().cast::<u8>() as usize;
        let top = bottom
            .checked_add(TRAP_STACK_SIZE)
            .expect("static trap-stack bounds must fit usize");
        (bottom, top)
    }
}

/// Trap-only hart state installed in `mscratch` after admission.
///
/// This is not a combined hart context. It contains only immutable trap-stack
/// bounds and policy plus `UnsafeCell` fields mutated by the current hart's
/// assembly entry/restore protocol. Upper code never receives this type.
#[repr(C, align(16))]
pub(super) struct HartTrapState {
    state: AtomicU32,
    stack_bottom: UnsafeCell<usize>,
    stack_top: UnsafeCell<usize>,
    current_frame: UnsafeCell<usize>,
    depth: UnsafeCell<usize>,
    flags: AtomicUsize,
    saved_sp: UnsafeCell<usize>,
    saved_t0: UnsafeCell<usize>,
    saved_t1: UnsafeCell<usize>,
    saved_t2: UnsafeCell<usize>,
    saved_t3: UnsafeCell<usize>,
    expected: ExpectedTrapRecord,
    handler: UnsafeCell<Option<&'static dyn TrapHandler>>,
    index: UnsafeCell<usize>,
    timer: UnsafeCell<Option<Arc<dyn TimerDevice>>>,
    counters: UnsafeCell<Option<PerformanceCounters>>,
}

// SAFETY: initialization has a one-writer Release/Acquire protocol. After it,
// immutable fields never change; mutable fields are hart-local `UnsafeCell`s
// governed by the bounded entry/restore state machine.
unsafe impl Sync for HartTrapState {}

impl HartTrapState {
    pub(super) const fn new() -> Self {
        Self {
            state: AtomicU32::new(TRAP_STATE_EMPTY),
            stack_bottom: UnsafeCell::new(0),
            stack_top: UnsafeCell::new(0),
            current_frame: UnsafeCell::new(0),
            depth: UnsafeCell::new(0),
            flags: AtomicUsize::new(0),
            saved_sp: UnsafeCell::new(0),
            saved_t0: UnsafeCell::new(0),
            saved_t1: UnsafeCell::new(0),
            saved_t2: UnsafeCell::new(0),
            saved_t3: UnsafeCell::new(0),
            expected: ExpectedTrapRecord::new(),
            handler: UnsafeCell::new(None),
            index: UnsafeCell::new(0),
            timer: UnsafeCell::new(None),
            counters: UnsafeCell::new(None),
        }
    }

    fn initialize(
        &self,
        stack: &TrapStack,
        handler: &'static dyn TrapHandler,
        index: usize,
        timer: Option<Arc<dyn TimerDevice>>,
        counters: Option<PerformanceCounters>,
    ) -> Result<(), TrapStateError> {
        self.state
            .compare_exchange(
                TRAP_STATE_EMPTY,
                TRAP_STATE_WRITING,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .map_err(|_| TrapStateError::AlreadyInitialized)?;
        let (bottom, top) = stack.bounds();
        if bottom % frame::FRAME_ALIGN != 0
            || top % frame::FRAME_ALIGN != 0
            || TRAP_STACK_SIZE < frame::FRAME_SIZE * 2 + frame::FRAME_ALIGN
        {
            fail_state_initialization(&self.state);
            return Err(TrapStateError::InvalidStack);
        }

        // SAFETY: this caller uniquely changed EMPTY to WRITING. Entry cannot
        // observe the fields before the final Release store, and no later code
        // mutates the immutable bounds or handler. The current hart may later
        // publish its probed H-metadata flag before lower-mode entry.
        unsafe {
            self.stack_bottom.get().write(bottom);
            self.stack_top.get().write(top);
            self.current_frame.get().write(0);
            self.depth.get().write(0);
            self.flags.store(0, Ordering::Relaxed);
            self.handler.get().write(Some(handler));
            self.index.get().write(index);
            self.timer.get().write(timer);
            self.counters.get().write(counters);
        }
        self.state.store(TRAP_STATE_READY, Ordering::Release);
        Ok(())
    }

    fn is_ready(&self) -> bool {
        self.state.load(Ordering::Acquire) == TRAP_STATE_READY
    }

    pub(super) fn expected(&self) -> &ExpectedTrapRecord {
        &self.expected
    }

    fn enable_hypervisor_metadata(&self) {
        // SAFETY: the caller is this state's current hart, with MIE disabled,
        // before its first lower-mode entry. Repeating this on a fresh HSM
        // start writes the same immutable capability value.
        unsafe { self.expected.enable_hypervisor_metadata() };
        self.flags.store(FLAG_HYPERVISOR, Ordering::Release);
    }

    pub(super) fn has_hypervisor_metadata(&self) -> bool {
        self.flags.load(Ordering::Acquire) & FLAG_HYPERVISOR != 0
    }

    fn index(&self) -> usize {
        // SAFETY: an Acquire-ready observation publishes this immutable field.
        unsafe { self.index.get().read() }
    }

    pub(super) fn read_time(&self) -> Option<u64> {
        // SAFETY: the optional Arc is written before Release publication and
        // never changed afterward. The stable driver owner is shared only
        // through its `Send + Sync` role.
        unsafe { (&*self.timer.get()).as_ref().map(|timer| timer.read_time()) }
    }

    fn prepare_timer(&self) -> Result<(), TimerError> {
        // SAFETY: the optional Arc is immutable after ready publication.
        unsafe {
            (&*self.timer.get())
                .as_ref()
                .map_or(Ok(()), |timer| timer.prepare_current_hart())
        }
    }

    pub(super) fn handle_timer_interrupt(&self) -> bool {
        // SAFETY: the optional Arc is immutable after ready publication.
        unsafe {
            (&*self.timer.get())
                .as_ref()
                .is_some_and(|timer| timer.handle_interrupt())
        }
    }

    /// Returns the entry assembly's currently published frame relation.
    pub(super) fn frame_publication(&self) -> (usize, usize, usize, usize) {
        // SAFETY: dispatch calls this only for the current hart's state after
        // activation; assembly owns mutation while machine interrupts are masked.
        unsafe {
            (
                *self.stack_bottom.get(),
                *self.stack_top.get(),
                *self.current_frame.get(),
                *self.depth.get(),
            )
        }
    }

    /// Returns the immutable upper trap handler installed before publication.
    pub(super) fn handler(&self) -> Option<&'static dyn TrapHandler> {
        // SAFETY: initialization writes the handler before the Release-ready
        // state and never changes it afterward.
        unsafe { *self.handler.get() }
    }

    fn abandon_current_frame(&self) -> Result<(), TrapStateError> {
        if !self.is_ready() {
            return Err(TrapStateError::InvalidIndex);
        }
        // SAFETY: only the current hart can call this terminal operation. A
        // depth-one frame is published until the successful checks below, and
        // the caller never returns to any Rust value that borrowed that frame.
        unsafe {
            if *self.depth.get() != 1 || *self.current_frame.get() == 0 {
                return Err(TrapStateError::InvalidIndex);
            }
            self.current_frame.get().write(0);
            self.depth.get().write(0);
        }
        Ok(())
    }
}

fn fail_state_initialization(state: &AtomicU32) {
    // The cell is intentionally left non-reusable. A failed preparation cannot
    // be repaired piecemeal and must prevent terminal runtime publication.
    state.store(TRAP_STATE_WRITING, Ordering::Release);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TrapStateError {
    AlreadyInitialized,
    InvalidStack,
    InvalidIndex,
    FeatureProbe,
}

#[used]
#[unsafe(link_section = ".bss.trap_stack")]
static TRAP_STACKS: [TrapStack; HART_CAPACITY] = [const { TrapStack::new() }; HART_CAPACITY];

#[used]
#[unsafe(link_section = ".bss.hart_trap_state")]
static HART_TRAP_STATES: [HartTrapState; HART_CAPACITY] =
    [const { HartTrapState::new() }; HART_CAPACITY];

pub(crate) fn prepare(
    index: usize,
    handler: &'static dyn TrapHandler,
    timer: Option<Arc<dyn TimerDevice>>,
    counters: Option<PerformanceCounters>,
) -> Result<(), TrapStateError> {
    let stack = TRAP_STACKS.get(index).ok_or(TrapStateError::InvalidIndex)?;
    let state = HART_TRAP_STATES
        .get(index)
        .ok_or(TrapStateError::InvalidIndex)?;
    state.initialize(stack, handler, index, timer, counters)
}

pub(crate) fn hypervisor_available(index: usize) -> bool {
    HART_TRAP_STATES
        .get(index)
        .is_some_and(|state| state.is_ready() && state.has_hypervisor_metadata())
}

pub(crate) fn prepare_timer(index: usize) -> Result<(), TimerError> {
    let state = HART_TRAP_STATES.get(index).ok_or(TimerError::InvalidHart)?;
    if !state.is_ready() {
        return Err(TimerError::InvalidHart);
    }
    state.prepare_timer()
}

pub(super) fn abort() -> ! {
    crate::power::abort(|| {})
}

const STATE_OFFSET: usize = core::mem::offset_of!(HartTrapState, state);
const STACK_BOTTOM_OFFSET: usize = core::mem::offset_of!(HartTrapState, stack_bottom);
const STACK_TOP_OFFSET: usize = core::mem::offset_of!(HartTrapState, stack_top);
const CURRENT_FRAME_OFFSET: usize = core::mem::offset_of!(HartTrapState, current_frame);
const DEPTH_OFFSET: usize = core::mem::offset_of!(HartTrapState, depth);
const FLAGS_OFFSET: usize = core::mem::offset_of!(HartTrapState, flags);
const SAVED_SP_OFFSET: usize = core::mem::offset_of!(HartTrapState, saved_sp);
const SAVED_T0_OFFSET: usize = core::mem::offset_of!(HartTrapState, saved_t0);
const SAVED_T1_OFFSET: usize = core::mem::offset_of!(HartTrapState, saved_t1);
const SAVED_T2_OFFSET: usize = core::mem::offset_of!(HartTrapState, saved_t2);
const SAVED_T3_OFFSET: usize = core::mem::offset_of!(HartTrapState, saved_t3);

const _: () = assert!(STATE_OFFSET == 0);
const _: () = assert!(TRAP_STACK_SIZE.is_multiple_of(frame::FRAME_ALIGN));
const _: () = assert!(core::mem::align_of::<HartTrapState>() == frame::FRAME_ALIGN);

#[cfg(test)]
#[path = "entry_tests.rs"]
mod tests;
