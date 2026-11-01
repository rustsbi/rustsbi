//! Hart start, stop, and suspend state transitions.

use crate::{Error, Result};
use alloc::boxed::Box;
use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicU8, Ordering};

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

#[repr(align(128))]
struct HartStateCell {
    state: AtomicU8,
    stage: UnsafeCell<Option<NextStage>>,
    transfer: UnsafeCell<Option<ControlTransfer>>,
}

impl HartStateCell {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(STATE_STOPPED),
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

/// Stages the initial boot handoff for the current hart.
pub fn stage_boot_handoff(stage: NextStage) -> Result<()> {
    let hart = current_hart();
    let cell = cell(hart);
    if cell
        .state
        .compare_exchange(
            STATE_STOPPED,
            STATE_STARTING,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .is_err()
    {
        return Err(Error::Busy);
    }
    // SAFETY: STARTING reserves the slot for this hart's boot path.
    unsafe { *cell.stage.get() = Some(stage) };
    cell.state.store(STATE_START_PENDING, Ordering::Release);
    Ok(())
}

/// Reserves and wakes a stopped hart, publishing its stage only after wake.
pub fn start(hart: HartId, stage: NextStage) -> Result<StartOutcome> {
    let cell = cell(hart);
    if cell
        .state
        .compare_exchange(
            STATE_STOPPED,
            STATE_STARTING,
            Ordering::Acquire,
            Ordering::Relaxed,
        )
        .is_err()
    {
        return Ok(StartOutcome::AlreadyRunning);
    }
    if let Err(error) = super::wakeup::wake(hart) {
        cell.state.store(STATE_STOPPED, Ordering::Release);
        return Err(error);
    }
    // SAFETY: STARTING reserves the slot until the Release publication.
    unsafe { *cell.stage.get() = Some(stage) };
    cell.state.store(STATE_START_PENDING, Ordering::Release);
    Ok(StartOutcome::Accepted)
}

/// Consumes a local start or park event.
pub(crate) fn take_local_event() -> HartEvent {
    let cell = cell(current_hart());
    loop {
        match cell.state.load(Ordering::Acquire) {
            STATE_START_PENDING => {
                if cell
                    .state
                    .compare_exchange(
                        STATE_START_PENDING,
                        STATE_STARTED,
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
    match cell(hart).state.load(Ordering::Acquire) {
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
/// Success discards its supervisor context and parks in M-mode until a new
/// start request arrives, including across spurious WFI wakeups.
/// Only failures return to the caller.
pub fn stop_current() -> Result<core::convert::Infallible> {
    let ipi = crate::ipi::get().ok_or(Error::IoError)?;
    ipi.delivery().ok_or(Error::IoError)?;
    crate::irq::with_local_disabled(|irq| {
        ipi.clear_current(irq)?;
        ipi.prepare_wakeup(irq);
        if let Some(timer) = crate::timer::get() {
            timer.cancel_events_on_stop(current_hart(), irq);
        }
        cell(current_hart())
            .state
            .store(STATE_STOPPED, Ordering::Release);
        // SAFETY:
        // 1. This scope keeps machine interrupts disabled until the handoff.
        // 2. Stop retires the current call chain before reusing the hart stack.
        unsafe {
            core::arch::asm!(
                "tail {finish}",
                finish = sym crate::boot::finish_boot,
                options(noreturn)
            )
        }
    })
}

/// Enters the platform suspend wait for the current hart.
pub fn suspend_current() -> Result<()> {
    let ipi = crate::ipi::get().ok_or(Error::IoError)?;
    ipi.delivery().ok_or(Error::IoError)?;
    crate::irq::with_local_disabled(|irq| {
        ipi.clear_current(irq)?;
        ipi.deliver_current(irq);
        ipi.prepare_wakeup(irq);
        Ok(())
    })?;
    cell(current_hart())
        .state
        .store(STATE_SUSPENDED, Ordering::Release);
    crate::instruction::wait_for_interrupt();
    Ok(())
}

/// Completes a retentive resume on the current hart.
pub fn resume_current_retentive() -> Result<()> {
    let cell = cell(current_hart());
    if cell
        .state
        .compare_exchange(
            STATE_SUSPENDED,
            STATE_STARTED,
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .is_err()
    {
        return Err(Error::InvalidState);
    }
    Ok(())
}

/// Reserves a non-retentive resume without exposing the cell itself.
pub fn begin_nonretentive_resume(stage: NextStage) -> Result<ResumeTicket> {
    let hart = current_hart();
    let cell = cell(hart);
    if cell
        .state
        .compare_exchange(
            STATE_SUSPENDED,
            STATE_RESUME_PENDING,
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .is_err()
    {
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
        crate::ipi::get()
            .ok_or(Error::IoError)?
            .send(crate::ipi::IpiRequest::for_hart(self.hart))?;
        // SAFETY:
        // 1. The ticket owns the cell while it is resume-pending.
        // 2. The same hart's ecall return path consumes the marker.
        unsafe {
            *self.cell.transfer.get() = Some(ControlTransfer::NonRetentiveResume(self.stage));
        }
        self.cell.state.store(STATE_STARTED, Ordering::Release);
        self.committed = true;
        Ok(())
    }
}

impl Drop for ResumeTicket {
    fn drop(&mut self) {
        if !self.committed {
            self.cell.state.store(STATE_SUSPENDED, Ordering::Release);
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
