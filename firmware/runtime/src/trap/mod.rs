//! Machine trap entry, dispatch, instruction emulation, and S/HS/VS forwarding.
//! Return state and original trap facts remain private to this subsystem.
//! Dispatch owns recovery and forwarding; emulation commits successful
//! instructions using the timer service or bounded lower-mode memory access.

mod decode;
mod dispatch;
mod emulate;
pub(crate) mod entry;
mod frame;
mod handlers;
mod init;
mod memory;
mod redirect;

pub use init::{InitError, init};

use csr::values::{Address, trap::Exception};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Fault {
    cause: Exception,
    address: Address,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Error {
    Unsupported,
    Fault(Fault),
}

use crate::context::{ExecutionContext, TransferError};

/// Stages a retentive control transfer for the current hart's ecall return
/// path: the hart's current execution is suspended into `suspend_into`,
/// and `resume_from` is entered when the pending ecall returns through the
/// dispatch.
///
/// The context state machine lives in [`crate::context`]; the ceremony
/// runs in the dispatch's ecall return path. Everything that can fail is
/// checked here; the transfer is consumed exactly once by the same hart,
/// and the committed ceremony cannot fail.
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
