//! Neutral execution contexts for retentive control transfers.
//!
//! A [`ExecutionContext`] is a plain-data snapshot of one suspendable
//! execution context. The monitor allocates one per context it wants to
//! keep suspendable, fills it, and parks it; Runtime defines the
//! representation, performs the switches, and never learns the monitor's
//! policy concepts (what a "domain", vCPU, or partition is, and which one
//! runs next).
//!
//! Runtime owns the whole transfer ceremony, on the hart's ecall return
//! path: it saves the outgoing register file, resume PC and translation
//! state into the suspend target, installs the resume source's declared
//! translation state, fences only when that state actually changes, loads
//! the incoming registers, and `mret`s. No client code executes inside the
//! sequence: the exchange is expressed entirely as data movement
//! between Runtime's private trap frame and the two context objects, so
//! the transfer's invariants hold by construction rather than by
//! convention. Staging is requested through
//! [`trap::stage_retentive_transfer`](crate::trap::stage_retentive_transfer).
//!
//! # Context states and ownership
//!
//! A context is [`Empty`](ContextState::Empty) until filled and parked,
//! [`Active`](ContextState::Active) while it is the hart's running
//! context, [`Suspended`](ContextState::Suspended) while it holds a valid
//! snapshot, and [`Staged`](ContextState::Staged) while Runtime owns it
//! inside an in-flight transaction. Only Runtime performs transitions,
//! atomically. A staged transfer hands both contexts to Runtime until its
//! commit; after the commit, the suspend target holds the outgoing
//! snapshot and the resume source is the hart's active context.
//!
//! # Exclusivity
//!
//! `Suspended → Staged` is a compare-and-swap on the context's state word,
//! so a snapshot can be staged by at most one hart at a time. An `Active`
//! context can only be suspended by the hart running it: staging verifies
//! the suspend target is the current hart's active context, or a fresh
//! `Empty` context when the hart has none recorded (its first retentive
//! transfer, adopting the currently running execution). No global lock is
//! involved.
//!
//! # Failure boundary
//!
//! Everything that can fail is checked when the transfer is staged; the
//! commit on the ecall return path is a fixed sequence of loads, CSR
//! writes and fences that cannot fail, so a staged transfer never needs
//! unwinding.
//!
//! # Fencing
//!
//! Runtime installs the resume source's declared translation state itself
//! and fences exactly when it differs from the outgoing state (an
//! `sfence.vma` after a changed `satp`; nothing when both sides are bare
//! or share the same `satp`). A monitor that modifies page tables under a
//! running context must order its own fences per the usual rules.
//!
//! # Hardware extensions
//!
//! Monitors built on extensions beyond the base CSR state (protection
//! domains, partitions, and similar) need no hook here. What such
//! extensions switch governs lower-privilege accesses, so M-mode monitor
//! code may run its own extension ceremony adjacent to this transaction,
//! before staging, while this module stays free of extension-specific
//! knowledge.

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicU8, Ordering};

/// The `satp` register value, re-exported so the public fields below name
/// their type from this module.
pub use riscv::register::satp::Satp;

/// Context state values; private Runtime facts transitioned only by the
/// functions in this module.
mod context_state {
    pub const EMPTY: u8 = 0;
    pub const SUSPENDED: u8 = 1;
    pub const STAGED: u8 = 2;
    pub const ACTIVE: u8 = 3;
}

/// The lifecycle states of an [`ExecutionContext`]; see the
/// [module documentation](self) for the ownership model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextState {
    /// Filled with no valid snapshot yet; may be filled and parked.
    Empty,
    /// Holds a valid snapshot, owned by monitor policy.
    Suspended,
    /// Owned by Runtime inside an in-flight transaction.
    Staged,
    /// Is the hart's running context; its data is stale by definition.
    Active,
}

/// A by-value copy of a context's saved data, for monitor introspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContextSnapshot {
    /// Registers x1-x31 at index `n - 1`; x0 is hardwired zero and has no
    /// slot.
    pub x1_x31: [usize; 31],
    /// The resume address.
    pub pc: usize,
    /// The translation state the context resumes with.
    pub satp: Satp,
}

/// A neutral, plain-data execution context exchanged by a retentive
/// control transfer.
///
/// The monitor allocates one per suspendable context (typically as a
/// `static`, since transfers stage and commit at different points of the
/// hart's trap path), fills it with [`fill`](Self::fill), and parks it
/// with [`park`](Self::park). All state transitions are performed by
/// Runtime; there is no way for monitor code to execute inside a
/// transfer.
// SAFETY: the data word is only written through methods gated on the
// atomic state word; every mutation path owns its state transition, so
// accesses are serialized by the state machine rather than by `&mut`.
pub struct ExecutionContext {
    data: UnsafeCell<ContextData>,
    state: AtomicU8,
}

unsafe impl Sync for ExecutionContext {}

impl Default for ExecutionContext {
    /// The empty context, same as [`new`](ExecutionContext::new).
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy)]
struct ContextData {
    /// Registers x1-x31 at index `n - 1`; x0 is hardwired zero and has no
    /// slot.
    x1_x31: [usize; 31],
    pc: usize,
    satp: Satp,
}

/// Failure while parking, filling, or staging a retentive transfer.
///
/// Every error is checked before any part of the transfer commits; a
/// staged transfer itself cannot fail.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransferError {
    /// The current hart is not in its started state.
    NotRunning,
    /// Another control transfer is already staged for this hart's ecall
    /// return path.
    Busy,
    /// The suspend target is neither the hart's active context nor a
    /// fresh empty context.
    InvalidSuspend,
    /// The resume source is not suspended (or lost the staging race to
    /// another hart).
    NotSuspended,
    /// The suspend target and the resume source are the same context.
    SameContext,
    /// The context is not in a state that accepts the requested
    /// operation (for example filling an active or staged context, or
    /// parking a non-empty one).
    InvalidState,
}

impl ExecutionContext {
    /// Creates an empty context, suitable for a `static`.
    pub const fn new() -> Self {
        ExecutionContext {
            data: UnsafeCell::new(ContextData {
                x1_x31: [0; 31],
                pc: 0,
                satp: Satp::from_bits(0),
            }),
            state: AtomicU8::new(context_state::EMPTY),
        }
    }

    /// Reads the context's current lifecycle state.
    pub fn state(&self) -> ContextState {
        match self.state.load(Ordering::Acquire) {
            context_state::EMPTY => ContextState::Empty,
            context_state::SUSPENDED => ContextState::Suspended,
            context_state::STAGED => ContextState::Staged,
            context_state::ACTIVE => ContextState::Active,
            _ => unreachable!("BUG: invalid Runtime context state"),
        }
    }

    /// Returns a by-value copy of the context's saved data.
    ///
    /// Reading is always well-defined; a context that is currently staged
    /// or active holds data in flux, and synchronizing against such a
    /// context is monitor policy.
    pub fn snapshot(&self) -> ContextSnapshot {
        // SAFETY: snapshot copies the data word; concurrent writers exist
        // only for staged contexts, which the caller synchronizes with.
        let data = unsafe { &*self.data.get() };
        ContextSnapshot {
            x1_x31: data.x1_x31,
            pc: data.pc,
            satp: data.satp,
        }
    }

    /// Fills the context's data, for the initial park or for a monitor
    /// update of an existing snapshot (for example writing return values
    /// into a suspended context before resuming it).
    ///
    /// Allowed only while the context is empty or suspended: an active
    /// context's data is defined by the running execution, and a staged
    /// context belongs to an in-flight transaction. Fills of a context
    /// that another hart may concurrently stage must be serialized by the
    /// monitor.
    pub fn fill(&self, x1_x31: [usize; 31], pc: usize, satp: Satp) -> Result<(), TransferError> {
        let state = self.state.load(Ordering::Acquire);
        if state != context_state::EMPTY && state != context_state::SUSPENDED {
            return Err(TransferError::InvalidState);
        }
        // SAFETY: empty or suspended contexts have no Runtime writer; the
        // monitor serializes its own concurrent fills.
        let data = unsafe { &mut *self.data.get() };
        *data = ContextData { x1_x31, pc, satp };
        Ok(())
    }

    /// Parks a filled context as suspended, making it eligible as a
    /// resume source.
    ///
    /// The context must be empty; a suspended context stays suspended
    /// until a transfer resumes it.
    pub fn park(&'static self) -> Result<(), TransferError> {
        self.state
            .compare_exchange(
                context_state::EMPTY,
                context_state::SUSPENDED,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .map(|_| ())
            .map_err(|_| TransferError::InvalidState)
    }

    /// Saves the outgoing execution into this context. Runtime-internal,
    /// called by the transfer ceremony on a context it owns as the staged
    /// suspend target.
    pub(crate) fn save_outgoing(&self, x1_x31: &[usize; 31], pc: usize, satp: Satp) {
        // SAFETY: the staged suspend target is owned by this hart's
        // in-flight transaction; no other accessor may touch it.
        let data = unsafe { &mut *self.data.get() };
        data.x1_x31.copy_from_slice(x1_x31);
        data.pc = pc;
        data.satp = satp;
    }

    /// Returns the declared translation state to install.
    pub(crate) fn satp(&self) -> Satp {
        // SAFETY: staged resume sources have no writer.
        unsafe { &*self.data.get() }.satp
    }

    /// Loads the incoming registers into the trap frame's x1-x31 slots and
    /// returns the entry PC for `mepc`. Runtime-internal, called by the
    /// transfer ceremony on a context it owns as the staged resume source.
    pub(crate) fn resume_into(&self, x1_x31: &mut [usize; 31]) -> usize {
        // SAFETY: staged resume sources have no writer.
        let data = unsafe { &*self.data.get() };
        x1_x31.copy_from_slice(&data.x1_x31);
        data.pc
    }
}

/// Stages the context pair of a retentive transfer: reserves the resume
/// source against other harts and moves both contexts to `Staged`.
///
/// This is the commit point of staging; on failure the states are rolled
/// back untouched.
pub(crate) fn stage_pair(
    suspend_into: &'static ExecutionContext,
    resume_from: &'static ExecutionContext,
) -> Result<(), TransferError> {
    if core::ptr::eq(suspend_into, resume_from) {
        return Err(TransferError::SameContext);
    }
    // Reserve the resume source first; losing the race means another
    // hart stages it.
    if resume_from
        .state
        .compare_exchange(
            context_state::SUSPENDED,
            context_state::STAGED,
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .is_err()
    {
        return Err(TransferError::NotSuspended);
    }
    let observed = suspend_into.state.load(Ordering::Acquire);
    if (observed == context_state::ACTIVE || observed == context_state::EMPTY)
        && suspend_into
            .state
            .compare_exchange(
                observed,
                context_state::STAGED,
                Ordering::AcqRel,
                Ordering::Relaxed,
            )
            .is_ok()
    {
        return Ok(());
    }
    // Roll the reservation back; the suspend target was not admissible.
    resume_from
        .state
        .store(context_state::SUSPENDED, Ordering::Release);
    Err(TransferError::InvalidSuspend)
}

/// Whether `context` may be adopted as the suspend target of a transfer on
/// a hart whose active context is `active`: the recorded active context
/// itself (currently running), or a fresh empty context when the hart
/// tracks none.
pub(crate) fn suspend_target_admissible(
    active: Option<&'static ExecutionContext>,
    context: &ExecutionContext,
) -> bool {
    match active {
        Some(active) => core::ptr::eq(active, context) && context.state() == ContextState::Active,
        None => context.state() == ContextState::Empty,
    }
}

/// Commits an in-flight transfer's context states: the suspend target
/// becomes `Suspended` with the outgoing snapshot, the resume source
/// becomes `Active`. Runtime-internal, called by the ceremony after the
/// data movement succeeds; it cannot fail.
pub(crate) fn commit_pair(
    suspend_into: &'static ExecutionContext,
    resume_from: &'static ExecutionContext,
) {
    suspend_into
        .state
        .store(context_state::SUSPENDED, Ordering::Release);
    resume_from
        .state
        .store(context_state::ACTIVE, Ordering::Release);
}

#[cfg(test)]
mod tests {
    use super::*;

    const GPRS_A: [usize; 31] = {
        let mut gprs = [0; 31];
        gprs[9] = 0x1111;
        gprs
    };
    const GPRS_B: [usize; 31] = {
        let mut gprs = [0; 31];
        gprs[9] = 0x2222;
        gprs
    };

    fn filled(pc: usize) -> ([usize; 31], usize, Satp) {
        (GPRS_B, pc, Satp::from_bits(0x8000))
    }

    #[test]
    fn park_requires_empty_and_fill_requires_parkable_state() {
        static CTX: ExecutionContext = ExecutionContext::new();
        CTX.fill(GPRS_A, 0x1000, Satp::from_bits(0)).unwrap();
        CTX.park().unwrap();
        assert_eq!(CTX.state(), ContextState::Suspended);
        assert_eq!(CTX.park(), Err(TransferError::InvalidState));
    }

    #[test]
    fn fill_rejected_while_staged_or_active() {
        static SUSPEND: ExecutionContext = ExecutionContext::new();
        static RESUME: ExecutionContext = ExecutionContext::new();
        RESUME.fill(GPRS_B, 0x2000, Satp::from_bits(0)).unwrap();
        RESUME.park().unwrap();
        stage_pair(&SUSPEND, &RESUME).unwrap();
        assert_eq!(SUSPEND.state(), ContextState::Staged);
        assert_eq!(RESUME.state(), ContextState::Staged);
        assert_eq!(
            SUSPEND.fill(GPRS_A, 1, Satp::from_bits(0)),
            Err(TransferError::InvalidState)
        );
        assert_eq!(
            RESUME.fill(GPRS_B, 1, Satp::from_bits(0)),
            Err(TransferError::InvalidState)
        );
        commit_pair(&SUSPEND, &RESUME);
        assert_eq!(SUSPEND.state(), ContextState::Suspended);
        assert_eq!(RESUME.state(), ContextState::Active);
        // A suspended snapshot may be updated; an active context may not.
        SUSPEND.fill(GPRS_A, 0x1001, Satp::from_bits(0)).unwrap();
        assert_eq!(
            RESUME.fill(GPRS_B, 1, Satp::from_bits(0)),
            Err(TransferError::InvalidState)
        );
    }

    #[test]
    fn stage_rejects_same_context() {
        static CTX: ExecutionContext = ExecutionContext::new();
        CTX.fill(GPRS_A, 0x1000, Satp::from_bits(0)).unwrap();
        CTX.park().unwrap();
        assert_eq!(stage_pair(&CTX, &CTX), Err(TransferError::SameContext));
    }

    #[test]
    fn stage_requires_suspended_resume_source() {
        static SUSPEND: ExecutionContext = ExecutionContext::new();
        static RESUME: ExecutionContext = ExecutionContext::new();
        // An empty (unparked) resume source is not a valid snapshot.
        assert_eq!(
            stage_pair(&SUSPEND, &RESUME),
            Err(TransferError::NotSuspended)
        );
    }

    #[test]
    fn staging_reservation_is_exclusive() {
        static FIRST: ExecutionContext = ExecutionContext::new();
        static SECOND: ExecutionContext = ExecutionContext::new();
        static RESUME: ExecutionContext = ExecutionContext::new();
        RESUME.fill(GPRS_B, 0x2000, Satp::from_bits(0)).unwrap();
        RESUME.park().unwrap();
        stage_pair(&FIRST, &RESUME).unwrap();
        assert_eq!(
            stage_pair(&SECOND, &RESUME),
            Err(TransferError::NotSuspended)
        );
        // The failed staging left no trace on either context.
        assert_eq!(FIRST.state(), ContextState::Staged);
        assert_eq!(RESUME.state(), ContextState::Staged);
        assert_eq!(SECOND.state(), ContextState::Empty);
    }

    #[test]
    fn suspend_target_must_be_active_or_empty() {
        static SUSPEND: ExecutionContext = ExecutionContext::new();
        static RESUME: ExecutionContext = ExecutionContext::new();
        SUSPEND.fill(GPRS_A, 0x1000, Satp::from_bits(0)).unwrap();
        RESUME.fill(GPRS_B, 0x2000, Satp::from_bits(0)).unwrap();
        SUSPEND.park().unwrap();
        RESUME.park().unwrap();
        // A suspended suspend target is meaningless: its data would be
        // overwritten by the outgoing snapshot.
        assert_eq!(
            stage_pair(&SUSPEND, &RESUME),
            Err(TransferError::InvalidSuspend)
        );
        // The resume reservation was rolled back.
        assert_eq!(RESUME.state(), ContextState::Suspended);
        assert_eq!(SUSPEND.state(), ContextState::Suspended);
    }

    #[test]
    fn admissible_suspend_targets() {
        static ACTIVE: ExecutionContext = ExecutionContext::new();
        static SNAPSHOT: ExecutionContext = ExecutionContext::new();
        static FRESH: ExecutionContext = ExecutionContext::new();
        SNAPSHOT.fill(GPRS_B, 0x2000, Satp::from_bits(0)).unwrap();
        SNAPSHOT.park().unwrap();
        ACTIVE.fill(GPRS_A, 0x1000, Satp::from_bits(0)).unwrap();
        ACTIVE.park().unwrap();
        stage_pair(&FRESH, &SNAPSHOT).unwrap();
        commit_pair(&FRESH, &SNAPSHOT);
        // SNAPSHOT is now the hart's active context.
        assert!(suspend_target_admissible(Some(&SNAPSHOT), &SNAPSHOT));
        assert!(!suspend_target_admissible(Some(&SNAPSHOT), &ACTIVE));
        assert!(!suspend_target_admissible(Some(&SNAPSHOT), &FRESH));
        // Without a tracked active context, only a fresh empty target may
        // adopt the running execution.
        static EMPTY: ExecutionContext = ExecutionContext::new();
        assert!(!suspend_target_admissible(None, &SNAPSHOT));
        assert!(suspend_target_admissible(None, &EMPTY));
    }

    #[test]
    fn full_cycle_saves_and_restores_as_data() {
        static HOST: ExecutionContext = ExecutionContext::new();
        static TSM: ExecutionContext = ExecutionContext::new();
        let (gprs, pc, satp) = filled(0x80400000);
        HOST.fill(GPRS_A, 0x80800000, Satp::from_bits(0x8080_0000))
            .unwrap();
        TSM.fill(gprs, pc, satp).unwrap();
        TSM.park().unwrap();

        // First transfer: adopt the running execution into the fresh HOST
        // context and enter TSM.
        stage_pair(&HOST, &TSM).unwrap();
        let outgoing = [7usize; 31];
        HOST.save_outgoing(&outgoing, 0x8080_0100, Satp::from_bits(0x8080_0000));
        let mut frame = [0usize; 31];
        let entry = TSM.resume_into(&mut frame);
        assert_eq!(entry, pc);
        assert_eq!(&frame[..], &GPRS_B[..]);
        commit_pair(&HOST, &TSM);

        let snapshot = HOST.snapshot();
        assert_eq!(snapshot.pc, 0x8080_0100);
        assert_eq!(snapshot.satp, Satp::from_bits(0x8080_0000));
        assert_eq!(snapshot.x1_x31[0], 7);
        assert_eq!(HOST.state(), ContextState::Suspended);
        assert_eq!(TSM.state(), ContextState::Active);

        // Second transfer: suspend TSM back into HOST's slot and resume
        // the saved HOST snapshot.
        stage_pair(&TSM, &HOST).unwrap();
        commit_pair(&TSM, &HOST);
        assert_eq!(TSM.state(), ContextState::Suspended);
        assert_eq!(HOST.state(), ContextState::Active);
        let entry = HOST.resume_into(&mut frame);
        // The saved snapshot is restored verbatim.
        assert_eq!(entry, 0x8080_0100);
        assert_eq!(&frame[..], &outgoing[..]);
    }
}
