//! Staging of retentive control transfers on the ecall return path.
//!
//! The context representation and its state machine live in
//! [`crate::context`]; this module binds them to the hart lifecycle and
//! the translation CSRs, and exposes the staging entry point. The
//! ceremony itself is performed by the dispatch's ecall return path.

use crate::context::{ExecutionContext, ProtectionState, TransferError};

impl ProtectionState {
    /// Reads the hart's current translation state.
    pub(crate) fn current() -> Self {
        let bits = riscv::register::satp::read().bits();
        if bits == 0 {
            ProtectionState::Bare
        } else {
            ProtectionState::Supervisor { satp: bits }
        }
    }

    /// Installs this state into the hart's `satp`.
    pub(crate) fn install(&self) {
        let bits = match *self {
            ProtectionState::Bare => 0,
            ProtectionState::Supervisor { satp } => satp,
        };
        // SAFETY: M-mode installs the staged transfer's declared
        // translation state on the current hart.
        unsafe { riscv::register::satp::write(riscv::register::satp::Satp::from_bits(bits)) };
    }
}

/// Stages a retentive control transfer for the current hart's ecall return
/// path: the hart's current execution is suspended into `suspend_into`,
/// and `resume_from` is entered when the pending ecall returns through the
/// dispatch.
///
/// Everything that can fail is checked here. On success the transfer is
/// consumed exactly once, by the same hart's ecall return path, and
/// cannot fail: the ceremony saves the outgoing context (registers, resume
/// PC, translation state) as data, installs the declared translation
/// state, fences only on an actual state change, loads the incoming
/// registers, and `mret`s. Monitor code never runs inside the ceremony.
///
/// # Errors
///
/// [`NotRunning`](TransferError::NotRunning) if the hart is not started,
/// [`Busy`](TransferError::Busy) if another transfer is already staged,
/// [`InvalidSuspend`](TransferError::InvalidSuspend) unless `suspend_into`
/// is the hart's active context or a fresh empty context,
/// [`NotSuspended`](TransferError::NotSuspended) unless `resume_from` is
/// suspended and wins the staging reservation, and
/// [`SameContext`](TransferError::SameContext) if both arguments name the
/// same context.
pub fn stage_retentive_transfer(
    suspend_into: &'static ExecutionContext,
    resume_from: &'static ExecutionContext,
) -> Result<(), TransferError> {
    let cell = crate::hart::current_cell();
    if !cell.is_started() {
        return Err(TransferError::NotRunning);
    }
    if cell.has_staged_transfer() {
        return Err(TransferError::Busy);
    }
    if !crate::context::suspend_target_admissible(cell.active_context(), suspend_into) {
        return Err(TransferError::InvalidSuspend);
    }
    crate::context::stage_pair(suspend_into, resume_from)?;
    cell.stage_retentive(suspend_into, resume_from);
    Ok(())
}
