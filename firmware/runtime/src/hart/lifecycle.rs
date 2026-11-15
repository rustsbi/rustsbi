//! Hart start, stop, and suspend state transitions.

use crate::boot::finish_boot;
use crate::{Error, Result, instruction, ipi, irq};
use alloc::boxed::Box;
use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicUsize, Ordering};

use super::HartId;
use crate::boot::NextStage;
use spin::Once;

// State values are private Runtime facts, not SBI state IDs.
const STATE_STARTED: u8 = 0;
const STATE_STOPPED: u8 = 1;
const STATE_START_PENDING: u8 = 2;
const STATE_SUSPENDED: u8 = 4;
const STATE_RESUME_PENDING: u8 = 6;
const STATE_STARTING: u8 = u8::MAX;

// The state word packs the state byte with a generation tag. Each start
// reservation bumps the tag and every other transition keeps the observed
// one, so a rollback CAS matches only the publication its own request made,
// never a later request's republished state.
#[inline]
const fn pack(state: u8, generation: usize) -> usize {
    (generation << 8) | state as usize
}

#[inline]
const fn state_of(word: usize) -> u8 {
    (word & 0xff) as u8
}

#[inline]
const fn generation_of(word: usize) -> usize {
    word >> 8
}

#[repr(align(128))]
struct HartStateCell {
    state: AtomicUsize,
    stage: UnsafeCell<Option<NextStage>>,
    transfer: UnsafeCell<Option<ControlTransfer>>,
}

impl HartStateCell {
    const fn new() -> Self {
        Self {
            state: AtomicUsize::new(pack(STATE_STOPPED, 0)),
            stage: UnsafeCell::new(None),
            transfer: UnsafeCell::new(None),
        }
    }
}

// SAFETY:
// 1. State publication is atomic.
// 2. Staged values are written only under a private reservation and consumed
//    by the owning hart.
unsafe impl Sync for HartStateCell {}

static HART_STATES: Once<Box<[HartStateCell]>> = Once::new();

/// Protocol-independent hart lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HartState {
    /// The hart is executing normally.
    Started,
    /// The hart is parked outside lower privilege modes.
    Stopped,
    /// A start handoff is being published or is waiting for the hart.
    StartPending,
    /// The hart is in a retentive platform suspend state.
    Suspended,
    /// A non-retentive resume is being prepared.
    ResumePending,
}

/// Work observed by the machine software/external interrupt transport.
pub(crate) enum HartEvent {
    /// A staged lower-privilege execution stage.
    Start(NextStage),
    /// A request to park the current hart until another start arrives.
    Park,
    /// No start or park event is pending.
    None,
}

/// Result of reserving a stopped hart for a start request.
pub enum StartOutcome {
    /// The request owns the start handoff.
    Accepted,
    /// The hart was not stopped and could not be reserved.
    AlreadyRunning,
}

/// A machine control transfer staged by a successful non-retentive resume.
pub(crate) enum ControlTransfer {
    /// A lower-privilege stage entered from the ecall return path.
    NonRetentiveResume(NextStage),
}

// Reserves a stopped hart for a start handoff and returns the request's
// generation tag.
fn reserve_start(cell: &HartStateCell) -> Option<usize> {
    let mut word = cell.state.load(Ordering::Relaxed);
    loop {
        if state_of(word) != STATE_STOPPED {
            return None;
        }
        let generation = generation_of(word).wrapping_add(1);
        match cell.state.compare_exchange(
            word,
            pack(STATE_STARTING, generation),
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some(generation),
            Err(current) => word = current,
        }
    }
}

/// Stages the initial boot handoff for the current hart.
pub fn stage_boot_handoff(stage: NextStage) -> Result<()> {
    let hart = current_hart();
    let cell = cell(hart);
    let Some(generation) = reserve_start(cell) else {
        return Err(Error::Busy);
    };
    // SAFETY: STARTING reserves the slot for this hart's boot path.
    unsafe { *cell.stage.get() = Some(stage) };
    cell.state
        .store(pack(STATE_START_PENDING, generation), Ordering::Release);
    Ok(())
}

/// Reserves a stopped hart, publishes its stage, and wakes it.
///
/// The stage write becomes visible through the Release publication of
/// `START_PENDING` before any wake backend runs, so a target that observes
/// the wake can never miss the staged handoff. A failed wake withdraws the
/// request only while its own publication is still current: each start
/// carries a generation tag, so a consumed or republished state is left
/// untouched.
pub fn start(hart: HartId, stage: NextStage) -> Result<StartOutcome> {
    let cell = cell(hart);
    let Some(generation) = reserve_start(cell) else {
        return Ok(StartOutcome::AlreadyRunning);
    };
    // SAFETY: STARTING reserves the slot until the Release publication.
    unsafe { *cell.stage.get() = Some(stage) };
    cell.state
        .store(pack(STATE_START_PENDING, generation), Ordering::Release);
    // Orders the stage and state publications before every wake backend,
    // covering both `HartWake` devices and IPI transports.
    instruction::fence();

    if let Err(error) = super::wakeup::wake(hart) {
        return match cell.state.compare_exchange(
            pack(STATE_START_PENDING, generation),
            pack(STATE_STOPPED, generation),
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            // The wake never became observable, so the request is withdrawn.
            Ok(_) => Err(error),
            // A different word means this publication was consumed, or a later
            // request already owns the slot; either way it stays untouched.
            Err(_) => Ok(StartOutcome::Accepted),
        };
    }

    Ok(StartOutcome::Accepted)
}

/// Consumes a local start or park event.
pub(crate) fn take_local_event() -> HartEvent {
    let cell = cell(current_hart());
    loop {
        let word = cell.state.load(Ordering::Acquire);
        match state_of(word) {
            STATE_START_PENDING => {
                if cell
                    .state
                    .compare_exchange(
                        word,
                        pack(STATE_STARTED, generation_of(word)),
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    )
                    .is_ok()
                {
                    // SAFETY:
                    // 1. The acquire-release transition consumes the published stage.
                    // 2. Only this hart consumes its local stage.
                    let stage = unsafe { (*cell.stage.get()).take() }
                        .expect("BUG: start state without staged handoff");
                    return HartEvent::Start(stage);
                }
            }
            STATE_STARTING => spin_loop(),
            STATE_STOPPED => return HartEvent::Park,
            _ => return HartEvent::None,
        }
    }
}

/// Reads a hart's protocol-independent lifecycle state.
#[inline]
pub fn status(hart: HartId) -> HartState {
    match state_of(cell(hart).state.load(Ordering::Acquire)) {
        STATE_STARTED => HartState::Started,
        STATE_STOPPED => HartState::Stopped,
        STATE_START_PENDING | STATE_STARTING => HartState::StartPending,
        STATE_SUSPENDED => HartState::Suspended,
        STATE_RESUME_PENDING => HartState::ResumePending,
        _ => unreachable!("BUG: invalid Runtime hart state"),
    }
}

/// Returns whether the hart can receive ordinary firmware IPIs.
#[inline]
pub fn can_receive_ipi(hart: HartId) -> bool {
    matches!(status(hart), HartState::Started | HartState::Suspended)
}

/// Stops the current hart.
///
/// Success discards its supervisor context and parks in M-mode until a new
/// start request arrives, including across spurious WFI wakeups.
/// Only failures return to the caller.
/// Returns [`Error::IoError`] if the firmware IPI device is unavailable.
pub fn stop_current() -> Result<core::convert::Infallible> {
    irq::with_local_disabled(|irq| {
        let ipi = ipi::get().ok_or(Error::IoError)?;
        ipi.acknowledge(irq)?;
        ipi.enable_interrupt(irq);
        let cell = cell(current_hart());
        let word = cell.state.load(Ordering::Relaxed);
        cell.state
            .store(pack(STATE_STOPPED, generation_of(word)), Ordering::Release);
        // SAFETY:
        // 1. This scope keeps machine interrupts disabled until the handoff.
        // 2. Stop retires the current call chain before reusing the hart stack.
        unsafe {
            core::arch::asm!(
                "tail {finish}",
                finish = sym finish_boot,
                options(noreturn)
            )
        }
    })
}

/// Enters the platform suspend wait for the current hart.
///
/// Returns [`Error::IoError`] if the firmware IPI device is unavailable.
pub fn suspend_current() -> Result<()> {
    irq::with_local_disabled(|irq| {
        let ipi = ipi::get().ok_or(Error::IoError)?;
        ipi.acknowledge(irq)?;
        ipi.process_pending(irq);
        ipi.enable_interrupt(irq);
        Ok(())
    })?;
    let cell = cell(current_hart());
    let word = cell.state.load(Ordering::Relaxed);
    cell.state.store(
        pack(STATE_SUSPENDED, generation_of(word)),
        Ordering::Release,
    );
    instruction::wait_for_interrupt();
    Ok(())
}

/// Completes a retentive resume on the current hart.
pub fn resume_current_retentive() -> Result<()> {
    let cell = cell(current_hart());
    let resumed = cell
        .state
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |word| {
            (state_of(word) == STATE_SUSPENDED).then(|| pack(STATE_STARTED, generation_of(word)))
        });
    if resumed.is_err() {
        return Err(Error::InvalidState);
    }
    Ok(())
}

/// Reserves a non-retentive resume without exposing the cell itself.
pub fn begin_nonretentive_resume(stage: NextStage) -> Result<ResumeTicket> {
    let hart = current_hart();
    let cell = cell(hart);
    let reserved = cell
        .state
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |word| {
            (state_of(word) == STATE_SUSPENDED)
                .then(|| pack(STATE_RESUME_PENDING, generation_of(word)))
        });
    if reserved.is_err() {
        return Err(Error::InvalidState);
    }
    Ok(ResumeTicket {
        hart,
        cell,
        stage,
        committed: false,
        _local: core::marker::PhantomData,
    })
}

/// A non-retentive resume reservation.
pub struct ResumeTicket {
    hart: HartId,
    cell: &'static HartStateCell,
    stage: NextStage,
    committed: bool,
    // A reservation must be completed or dropped on the hart that created it.
    _local: core::marker::PhantomData<*mut ()>,
}

impl ResumeTicket {
    /// Wakes the hart and publishes its lower-privilege handoff.
    pub fn commit(mut self) -> Result<()> {
        ipi::get()
            .ok_or(Error::IoError)?
            .send_hardware_ipi(self.hart)?;
        // SAFETY:
        // 1. The ticket owns the cell while it is resume-pending.
        // 2. The same hart's ecall return path consumes the marker.
        unsafe {
            *self.cell.transfer.get() = Some(ControlTransfer::NonRetentiveResume(self.stage));
        }
        let word = self.cell.state.load(Ordering::Relaxed);
        self.cell
            .state
            .store(pack(STATE_STARTED, generation_of(word)), Ordering::Release);
        self.committed = true;
        Ok(())
    }
}

impl Drop for ResumeTicket {
    fn drop(&mut self) {
        if !self.committed {
            let word = self.cell.state.load(Ordering::Relaxed);
            self.cell.state.store(
                pack(STATE_SUSPENDED, generation_of(word)),
                Ordering::Release,
            );
        }
    }
}

/// Takes the one-shot machine control transfer marker for the dispatch hart.
pub(crate) fn take_control_transfer(hart: HartId) -> Option<ControlTransfer> {
    assert_eq!(hart, current_hart());
    // SAFETY: The identity belongs to the calling hart, which consumes only
    // its own marker in the M-mode ecall return path.
    unsafe { (*cell(hart).transfer.get()).take() }
}

#[inline]
fn cell(hart: HartId) -> &'static HartStateCell {
    &HART_STATES.call_once(|| HartId::all().map(|_| HartStateCell::new()).collect())[hart.index()]
}

#[inline]
pub(crate) fn current_hart() -> HartId {
    HartId::current().expect("BUG: current hart is not in the boot topology")
}
