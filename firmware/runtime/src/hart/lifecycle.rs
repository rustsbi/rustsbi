//! Hart start, stop, and suspend state transitions.

use crate::{Error, Result};
use alloc::boxed::Box;
use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicU8, Ordering};

use super::HartId;
use crate::boot::NextStage;
use crate::context::ExecutionContext;
use spin::Once;

// State values are private Runtime facts, not SBI state IDs.
const STATE_STARTED: u8 = 0;
const STATE_STOPPED: u8 = 1;
const STATE_START_PENDING: u8 = 2;
const STATE_SUSPENDED: u8 = 4;
const STATE_RESUME_PENDING: u8 = 6;
const STATE_STARTING: u8 = u8::MAX;

#[repr(align(128))]
pub(crate) struct HartStateCell {
    state: AtomicU8,
    stage: UnsafeCell<Option<NextStage>>,
    transfer: UnsafeCell<Option<ControlTransfer>>,
    active_context: UnsafeCell<Option<&'static ExecutionContext>>,
}

impl HartStateCell {
    const fn new() -> Self {
        Self {
            state: AtomicU8::new(STATE_STOPPED),
            stage: UnsafeCell::new(None),
            transfer: UnsafeCell::new(None),
            active_context: UnsafeCell::new(None),
        }
    }

    /// Whether the hart is executing normally.
    pub(crate) fn is_started(&self) -> bool {
        self.state.load(Ordering::Acquire) == STATE_STARTED
    }

    /// Whether an ecall return-path control transfer is pending.
    pub(crate) fn has_staged_transfer(&self) -> bool {
        // SAFETY: only the owning hart's ecall path stages or consumes
        // its transfer marker.
        unsafe { (*self.transfer.get()).is_some() }
    }

    /// The context the hart is currently executing, if a retentive
    /// transfer has recorded one.
    pub(crate) fn active_context(&self) -> Option<&'static ExecutionContext> {
        // SAFETY: the active-context word is written only by this hart's
        // own ecall return path and stop, and read by the same hart.
        unsafe { *self.active_context.get() }
    }

    /// Records the hart's active context (or clears the record).
    pub(crate) fn set_active_context(&self, context: Option<&'static ExecutionContext>) {
        // SAFETY: see `active_context`.
        unsafe { *self.active_context.get() = context };
    }

    /// Publishes a staged retentive transfer. Infallible by construction:
    /// callers verify the hart is started, owns no staged transfer, and
    /// have already staged the context pair.
    pub(crate) fn stage_retentive(
        &self,
        suspend_into: &'static ExecutionContext,
        resume_from: &'static ExecutionContext,
    ) {
        // SAFETY: the current hart owns its transfer slot on its own
        // M-mode ecall path; the marker is consumed once by the same
        // hart's return path.
        unsafe {
            *self.transfer.get() = Some(ControlTransfer::Retentive {
                suspend_into,
                resume_from,
            })
        };
    }
}

// SAFETY: state publication is atomic; staged values are only written while
// a private reservation is held and are consumed by the owning hart.
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
    /// Enter the staged lower-privilege execution stage.
    Start(NextStage),
    /// Park the current hart until another start arrives.
    Park,
    /// Deliver ordinary platform software-interrupt work.
    None,
}

/// Result of reserving a stopped hart for a start request.
pub enum StartOutcome {
    /// The request owns the start handoff.
    Accepted,
    /// The hart was not stopped and could not be reserved.
    AlreadyRunning,
}

/// A machine control transfer staged for the ecall return path.
pub(crate) enum ControlTransfer {
    /// Enter the supplied lower-privilege stage from the ecall return path.
    NonRetentiveResume(NextStage),
    /// Switch execution contexts retentively from the ecall return path:
    /// the outgoing execution is saved into the suspend target and the
    /// resume source is entered, both as plain data movement performed by
    /// Runtime alone.
    Retentive {
        suspend_into: &'static ExecutionContext,
        resume_from: &'static ExecutionContext,
    },
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
                    // SAFETY: the AcqRel transition consumes the published
                    // stage, and only this hart consumes its local stage.
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
    let ipi = crate::boot::ipi().ok_or(Error::IoError)?;
    ipi.delivery().ok_or(Error::IoError)?;
    crate::irq::with_local_disabled(|irq| {
        ipi.clear_current(irq)?;
        ipi.prepare_wakeup(irq);
        Ok(())
    })?;
    // Stopping discards the supervisor context, so its tracking goes too.
    set_active_context(None);
    cell(current_hart())
        .state
        .store(STATE_STOPPED, Ordering::Release);
    // SAFETY: M-mode interrupts remain disabled. Stop retires the current
    // trap call chain, allowing the Runtime finisher to reuse the clean stack
    // and wait for a new start without returning to the stopped supervisor.
    unsafe {
        core::arch::asm!(
            "tail {finish}",
            finish = sym crate::boot::finish_boot,
            options(noreturn)
        )
    }
}

/// Enters the platform suspend wait for the current hart.
pub fn suspend_current() -> Result<()> {
    let ipi = crate::boot::ipi().ok_or(Error::IoError)?;
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
    riscv::asm::wfi();
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
        crate::boot::ipi()
            .ok_or(Error::IoError)?
            .send(crate::ipi::IpiRequest::for_hart(self.hart))?;
        // SAFETY: the ticket owns the cell while it is ResumePending; this
        // marker is consumed by the same hart's ecall return path.
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
    // SAFETY: the identity belongs to the calling hart, which consumes only
    // its own marker in the M-mode ecall return path.
    unsafe { (*cell(hart).transfer.get()).take() }
}

/// Returns the current hart's lifecycle cell for transfer staging.
pub(crate) fn current_cell() -> &'static HartStateCell {
    cell(current_hart())
}

/// Records (or clears) the current hart's active execution context.
pub(crate) fn set_active_context(context: Option<&'static ExecutionContext>) {
    cell(current_hart()).set_active_context(context);
}

#[inline]
fn cell(hart: HartId) -> &'static HartStateCell {
    &HART_STATES.call_once(|| HartId::all().map(|_| HartStateCell::new()).collect())[hart.index()]
}

#[inline]
pub(crate) fn current_hart() -> HartId {
    HartId::current().expect("BUG: current hart is not in the boot topology")
}
