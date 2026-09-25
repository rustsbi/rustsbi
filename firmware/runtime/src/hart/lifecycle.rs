//! Hart start, stop, and suspend state transitions.

use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicU8, Ordering};

use super::HartId;
use crate::boot::NextStage;
use crate::cfg::NUM_HART_MAX;

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

// SAFETY: state publication is atomic; staged values are only written while
// a private reservation is held and are consumed by the owning hart.
unsafe impl Sync for HartStateCell {}

static HART_STATES: [HartStateCell; NUM_HART_MAX] = [const { HartStateCell::new() }; NUM_HART_MAX];

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

/// Failure while publishing a start handoff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StartError {
    /// The platform wake operation failed; the state was rolled back.
    WakeFailed,
}

/// Failure while staging the initial boot handoff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StageError {
    /// The current hart was not in its initial stopped state.
    Busy,
    /// The current hart was not in its started state.
    NotRunning,
}

/// Failure while entering or leaving suspend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SuspendError {
    /// The platform software-interrupt state could not be cleared.
    Platform,
}

/// Failure while resuming a suspended hart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResumeError {
    /// The hart was not in the expected suspended state.
    InvalidState,
    /// The platform wake operation failed and the state was rolled back.
    WakeFailed,
}

/// A machine control transfer staged for the ecall return path.
pub(crate) enum ControlTransfer {
    /// Enter the supplied lower-privilege stage from the ecall return path.
    NonRetentiveResume(NextStage),
    /// Switch execution contexts retentively from the ecall return path: the
    /// outgoing context is saved, the incoming context is loaded into the
    /// frame, and the normal ecall-return ceremony enters the incoming one.
    RetentiveResume(&'static dyn DomainContext),
}

/// A client-owned saved execution context exchanged by a retentive control
/// transfer.
///
/// A domain switch must save the outgoing context and load an incoming one
/// without duplicating Runtime's register-lifetime protocol: the GPR file is
/// exchanged through this trait, and Runtime copies it into and out of its
/// private trap frame. The implementor decides where saved contexts live;
/// Runtime decides when the exchange happens and keeps the frame private.
///
/// The trait is consulted on the ecall return path of the hart that staged
/// the transfer, with interrupts disabled in M-mode.
pub trait DomainContext: Sync {
    /// Receives the outgoing context's GPR file and resume PC. The GPR file
    /// is indexed by register number, with `gprs[0]` hardwired to zero. The
    /// PC is the outgoing context's live resume address at the point of the
    /// transfer (on the ecall return path, the instruction after the ecall).
    fn save_outgoing(&self, gprs: &[usize; 32], pc: usize);
    /// Fills the incoming context's GPR file (indexed by register number;
    /// `gprs[0]` is ignored on restore) and returns the incoming context's
    /// entry PC, which Runtime programs into `mepc` for the `mret`.
    ///
    /// Runs after Runtime has staged its entry trap-state reset, so the
    /// implementation may restore incoming S-mode CSRs (such as `satp`) that
    /// the reset cleared. Runtime flushes the local TLB after this call
    /// returns, so an implementation may switch `satp` (including ASID)
    /// without issuing its own `sfence.vma`.
    fn restore_incoming(&self, gprs: &mut [usize; 32]) -> usize;
}

/// Stages the initial boot handoff for the current hart.
pub fn stage_current(stage: NextStage) -> Result<(), StageError> {
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
        return Err(StageError::Busy);
    }
    // SAFETY: STARTING reserves the slot for this hart's boot path.
    unsafe { *cell.stage.get() = Some(stage) };
    cell.state.store(STATE_START_PENDING, Ordering::Release);
    Ok(())
}

/// Stages a retentive control transfer for the current hart's ecall return
/// path. The transfer is consumed once, by the same hart, when its pending
/// ecall returns through the dispatch.
pub fn stage_retentive_transfer(context: &'static dyn DomainContext) -> Result<(), StageError> {
    let cell = cell(current_hart());
    if cell.state.load(Ordering::Acquire) != STATE_STARTED {
        return Err(StageError::NotRunning);
    }
    // SAFETY: the current hart owns its transfer slot on its own M-mode ecall
    // path; the marker is consumed once by the same hart's return path.
    unsafe { *cell.transfer.get() = Some(ControlTransfer::RetentiveResume(context)) };
    Ok(())
}

/// Reserves and wakes a stopped hart, publishing its stage only after wake.
pub fn start(hart: HartId, stage: NextStage) -> Result<StartOutcome, StartError> {
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

/// Failure to acknowledge the interrupt source before stopping a hart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopError {
    /// The platform IPI device is absent or could not clear its source.
    Platform,
}

/// Stops the current hart. Success discards its supervisor context and parks
/// in M-mode until a new start request arrives, including across spurious WFI
/// wakeups. Only failures return to the caller.
pub fn stop_current() -> Result<core::convert::Infallible, StopError> {
    let ipi = crate::ipi::get().ok_or(StopError::Platform)?;
    ipi.clear_current().map_err(|_| StopError::Platform)?;
    crate::csr::mie::set_machine_software();
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
pub fn suspend_current() -> Result<(), SuspendError> {
    let ipi = crate::ipi::get().ok_or(SuspendError::Platform)?;
    if ipi.clear_current().is_err() {
        return Err(SuspendError::Platform);
    }
    crate::ipi::handler()
        .expect("BUG: IPI handler not published")
        .deliver_current();
    crate::csr::mie::set_machine_software();
    cell(current_hart())
        .state
        .store(STATE_SUSPENDED, Ordering::Release);
    riscv::asm::wfi();
    Ok(())
}

/// Completes a retentive resume on the current hart.
pub fn resume_current_retentive() -> Result<(), ResumeError> {
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
        return Err(ResumeError::InvalidState);
    }
    Ok(())
}

/// Reserves a non-retentive resume without exposing the cell itself.
pub fn begin_nonretentive_resume(stage: NextStage) -> Result<ResumeTicket, ResumeError> {
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
        return Err(ResumeError::InvalidState);
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
    pub fn commit(mut self) -> Result<(), ResumeError> {
        let Some(wake) = crate::ipi::get() else {
            return Err(ResumeError::WakeFailed);
        };
        if wake.send(self.hart).is_err() {
            return Err(ResumeError::WakeFailed);
        }
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

/// Takes the current hart's one-shot machine control transfer marker.
pub(crate) fn take_control_transfer() -> Option<ControlTransfer> {
    // SAFETY: only the current hart's M-mode ecall path consumes this marker.
    unsafe { (*cell(current_hart()).transfer.get()).take() }
}

#[inline]
fn cell(hart: HartId) -> &'static HartStateCell {
    &HART_STATES[hart.0]
}

#[inline]
pub(crate) fn current_hart() -> HartId {
    HartId::current().expect("BUG: current hart exceeds Runtime capacity")
}
