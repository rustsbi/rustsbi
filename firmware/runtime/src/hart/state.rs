//! Hart start, stop, and suspend state transitions.

use crate::{Error, Result};
use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::sync::atomic::{AtomicU8, Ordering};

use super::{Hart, HartId};
use crate::boot::NextStage;
use crate::irq::LocalIrqDisabled;

/// Complete Runtime hart lifecycle state, including private reservations.
///
/// These are internal Runtime facts, not SBI state IDs. [`Self::Starting`] is a
/// reservation held while a stopped hart's handoff is staged and before
/// [`Self::StartPending`] is published; it is never a public [`HartState`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LifecycleState {
    /// The hart is executing normally.
    Started,
    /// The hart is parked outside lower privilege modes.
    Stopped,
    /// A start handoff is published and awaits the hart.
    StartPending,
    /// A stop is committed and awaits retirement of the policy call.
    StopPending,
    /// The hart is in a retentive platform suspend state.
    Suspended,
    /// A non-retentive resume is being prepared.
    ResumePending,
    /// The hart is reserved for a start or boot handoff before publication.
    ///
    /// This is an unpublished handoff state: the owning path writes the staged
    /// entry while the reservation is held, then publishes [`Self::StartPending`].
    Starting,
}

impl LifecycleState {
    /// Encodes the state for the atomic word, preserving the former numeric values.
    const fn to_u8(self) -> u8 {
        match self {
            Self::Started => 0,
            Self::Stopped => 1,
            Self::StartPending => 2,
            Self::StopPending => 3,
            Self::Suspended => 4,
            Self::ResumePending => 6,
            Self::Starting => u8::MAX,
        }
    }

    /// Decodes an atomic word with a checked match; unencoded values are bugs.
    fn from_u8(raw: u8) -> Self {
        match raw {
            0 => Self::Started,
            1 => Self::Stopped,
            2 => Self::StartPending,
            3 => Self::StopPending,
            4 => Self::Suspended,
            6 => Self::ResumePending,
            u8::MAX => Self::Starting,
            _ => unreachable!("BUG: invalid Runtime hart state"),
        }
    }

    /// Maps the internal state to its public lifecycle view.
    const fn public(self) -> HartState {
        match self {
            Self::Started => HartState::Started,
            Self::Stopped => HartState::Stopped,
            Self::StartPending | Self::Starting => HartState::StartPending,
            Self::StopPending => HartState::StopPending,
            Self::Suspended => HartState::Suspended,
            Self::ResumePending => HartState::ResumePending,
        }
    }
}

/// Hart lifecycle state stored atomically with checked enum conversion.
struct AtomicLifecycleState(AtomicU8);

impl AtomicLifecycleState {
    const fn new(state: LifecycleState) -> Self {
        Self(AtomicU8::new(state.to_u8()))
    }

    fn load(&self, order: Ordering) -> LifecycleState {
        LifecycleState::from_u8(self.0.load(order))
    }

    fn store(&self, state: LifecycleState, order: Ordering) {
        self.0.store(state.to_u8(), order);
    }

    fn compare_exchange(
        &self,
        current: LifecycleState,
        new: LifecycleState,
        success: Ordering,
        failure: Ordering,
    ) -> core::result::Result<LifecycleState, LifecycleState> {
        self.0
            .compare_exchange(current.to_u8(), new.to_u8(), success, failure)
            .map(LifecycleState::from_u8)
            .map_err(LifecycleState::from_u8)
    }
}

#[repr(align(128))]
pub(super) struct HartStateCell {
    state: AtomicLifecycleState,
    stage: UnsafeCell<Option<NextStage>>,
    transfer: UnsafeCell<Option<ControlTransfer>>,
}

impl HartStateCell {
    pub(super) const fn new() -> Self {
        Self {
            state: AtomicLifecycleState::new(LifecycleState::Stopped),
            stage: UnsafeCell::new(None),
            transfer: UnsafeCell::new(None),
        }
    }
}

// SAFETY: state publication is atomic; staged values are only written while
// a private reservation is held and are consumed by the owning hart.
unsafe impl Sync for HartStateCell {}

/// Protocol-independent hart lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HartState {
    /// The hart is executing normally.
    Started,
    /// The hart is parked outside lower privilege modes.
    Stopped,
    /// A start handoff is being published or is waiting for the hart.
    StartPending,
    /// A stop is committed and awaits retirement of the policy call.
    StopPending,
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
    /// The request acquired the start reservation; the hart may not have started yet.
    Accepted,
    /// The hart was not stopped, so the request did not acquire the reservation.
    NotStopped(HartState),
}

/// A machine control transfer staged by a successful non-retentive resume.
pub(crate) enum ControlTransfer {
    /// Retire the supervisor context after the SBI policy call has returned.
    Stop,
    /// Enter the supplied lower-privilege stage from the ecall return path.
    NonRetentiveResume(NextStage),
}

impl Hart {
    /// Stages the initial boot handoff for the current hart.
    pub fn stage_boot_handoff(&self, stage: NextStage) -> Result<()> {
        self.require_current()?;
        let cell = &self.state;
        if cell
            .state
            .compare_exchange(
                LifecycleState::Stopped,
                LifecycleState::Starting,
                Ordering::Acquire,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return Err(Error::Busy);
        }
        // SAFETY: Starting reserves the slot for this hart's boot path.
        unsafe { *cell.stage.get() = Some(stage) };
        cell.state
            .store(LifecycleState::StartPending, Ordering::Release);
        Ok(())
    }

    /// Reserves and wakes a stopped hart, publishing its stage only after wake.
    pub fn start(
        &self,
        stage: NextStage,
        wake: Option<&dyn super::HartWake>,
    ) -> Result<StartOutcome> {
        let cell = &self.state;
        if let Err(state) = cell.state.compare_exchange(
            LifecycleState::Stopped,
            LifecycleState::Starting,
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            return Ok(StartOutcome::NotStopped(state.public()));
        }
        if let Err(error) = super::wakeup::wake(self.id(), wake) {
            cell.state.store(LifecycleState::Stopped, Ordering::Release);
            return Err(error);
        }
        // SAFETY: Starting reserves the slot until the Release publication.
        unsafe { *cell.stage.get() = Some(stage) };
        cell.state
            .store(LifecycleState::StartPending, Ordering::Release);
        Ok(StartOutcome::Accepted)
    }

    /// Consumes a local start or park event.
    pub(crate) fn take_local_event(&self) -> HartEvent {
        assert_eq!(self.id(), super::current_hart());
        let cell = &self.state;
        loop {
            match cell.state.load(Ordering::Acquire) {
                LifecycleState::StartPending => {
                    if cell
                        .state
                        .compare_exchange(
                            LifecycleState::StartPending,
                            LifecycleState::Started,
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
                LifecycleState::Starting => spin_loop(),
                LifecycleState::Stopped => return HartEvent::Park,
                LifecycleState::Started
                | LifecycleState::StopPending
                | LifecycleState::Suspended
                | LifecycleState::ResumePending => return HartEvent::None,
            }
        }
    }

    /// Reads a hart's protocol-independent lifecycle state.
    #[inline]
    pub fn status(&self) -> HartState {
        self.state.state.load(Ordering::Acquire).public()
    }

    /// Returns whether the hart can receive ordinary firmware IPIs.
    #[inline]
    pub fn can_receive_ipi(&self) -> bool {
        matches!(self.status(), HartState::Started | HartState::Suspended)
    }

    /// Requests a stop at the enclosing SBI call's return to Runtime.
    /// Policy must return after success so its Rust call chain can unwind normally.
    /// Runtime then parks until a new start without returning to the old supervisor.
    pub fn stop(&self) -> Result<()> {
        self.require_current()?;
        let ipi = crate::ipi::get().ok_or(Error::IoError)?;
        ipi.delivery().ok_or(Error::IoError)?;
        crate::irq::with_local_disabled(|irq| {
            self.state
                .state
                .compare_exchange(
                    LifecycleState::Started,
                    LifecycleState::StopPending,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .map_err(|_| Error::InvalidState)?;
            if let Err(error) = ipi.clear_current(irq) {
                self.state
                    .state
                    .store(LifecycleState::Started, Ordering::Release);
                return Err(error);
            }
            ipi.prepare_wakeup(irq);
            // SAFETY: This hart owns its transfer slot and excludes IRQ reentry.
            unsafe { *self.state.transfer.get() = Some(ControlTransfer::Stop) };
            Ok(())
        })
    }

    /// Enters the platform suspend wait for the current hart.
    pub fn suspend(&self) -> Result<()> {
        self.require_current()?;
        let ipi = crate::ipi::get().ok_or(Error::IoError)?;
        ipi.delivery().ok_or(Error::IoError)?;
        crate::irq::with_local_disabled(|irq| {
            self.state
                .state
                .compare_exchange(
                    LifecycleState::Started,
                    LifecycleState::Suspended,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .map_err(|_| Error::InvalidState)?;
            ipi.clear_current(irq)?;
            let notified = ipi.handle_current(irq);
            ipi.prepare_wakeup(irq);
            // An IPI consumed above has already become SSIP. It must still
            // wake this suspend call when the supervisor has disabled SSIE.
            if !notified {
                riscv::asm::wfi();
            }
            Ok(())
        })
        .inspect_err(|_| {
            let _ = self.state.state.compare_exchange(
                LifecycleState::Suspended,
                LifecycleState::Started,
                Ordering::AcqRel,
                Ordering::Relaxed,
            );
        })
    }

    /// Completes a retentive resume on the current hart.
    pub fn resume_retentive(&self) -> Result<()> {
        self.require_current()?;
        let cell = &self.state;
        if cell
            .state
            .compare_exchange(
                LifecycleState::Suspended,
                LifecycleState::Started,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return Err(Error::InvalidState);
        }
        Ok(())
    }

    /// Commits a non-retentive handoff after the platform wait has returned.
    /// A failed wake returns to the original supervisor with Started restored.
    pub fn resume_nonretentive(&self, stage: NextStage) -> Result<()> {
        self.require_current()?;
        crate::irq::with_local_disabled(|_| {
            self.state
                .state
                .compare_exchange(
                    LifecycleState::Suspended,
                    LifecycleState::ResumePending,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                )
                .map_err(|_| Error::InvalidState)?;
            let result = crate::ipi::get()
                .ok_or(Error::IoError)
                .and_then(|ipi| ipi.send(crate::ipi::IpiRequest::for_hart(self.id())));
            if result.is_ok() {
                // SAFETY: This hart owns its transfer slot with IRQs excluded.
                unsafe {
                    *self.state.transfer.get() = Some(ControlTransfer::NonRetentiveResume(stage))
                };
            }
            self.state
                .state
                .store(LifecycleState::Started, Ordering::Release);
            result
        })
    }

    fn require_current(&self) -> Result<()> {
        if self.id() != HartId::current()? {
            return Err(Error::InvalidArgs);
        }
        if !crate::boot::traps_ready() || usize::from(csr::machine::Scratch::read()) != 0 {
            return Err(Error::NotReady);
        }
        Ok(())
    }

    /// Waits on this hart's transport without discarding live Rust stack frames.
    pub(crate) fn wait_for_start(&self, irq: &LocalIrqDisabled) -> NextStage {
        let ipi = crate::ipi::get().expect("IPI service not published");
        if self.state.state.load(Ordering::Acquire) == LifecycleState::StopPending {
            self.state
                .state
                .store(LifecycleState::Stopped, Ordering::Release);
        }
        loop {
            if ipi.delivery().is_some() {
                ipi.clear_current(irq)
                    .expect("could not clear hart wake interrupt");
                riscv::asm::fence();
            }
            let event = self.take_local_event();
            ipi.handle_current(irq);
            match event {
                HartEvent::Start(stage) => return stage,
                HartEvent::Park => {
                    ipi.prepare_wakeup(irq);
                    riscv::asm::wfi();
                }
                HartEvent::None => unreachable!("waiting hart is neither stopped nor starting"),
            }
        }
    }

    /// Takes the committed resume marker on this hart's ecall return path.
    pub(crate) fn take_control_transfer(&self) -> Option<ControlTransfer> {
        assert_eq!(self.id(), super::current_hart());
        // SAFETY: Only this hart consumes its own marker in M-mode.
        unsafe { (*self.state.transfer.get()).take() }
    }
}
