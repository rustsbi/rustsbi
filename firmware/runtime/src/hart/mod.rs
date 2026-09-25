//! RISC-V hart identity, sets, local storage, and lifecycle.
//!
//! SBI HSM policy and ABI translation belong to the firmware.

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod lifecycle;
mod local;
mod set;
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod wakeup;

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub use wakeup::{HartWake, install_wakeup};

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(crate) use lifecycle::{
    ControlTransfer, HartEvent, current_hart, take_control_transfer, take_local_event,
};
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub use lifecycle::{
    DomainContext, HartState, ResumeError, ResumeTicket, StageError, StartError, StartOutcome,
    StopError, SuspendError, begin_nonretentive_resume, can_receive_ipi, resume_current_retentive,
    stage_current, stage_retentive_transfer, start, status, stop_current, suspend_current,
};
pub use local::{HartLocal, HartLocalError};
pub use set::{HartSet, HartSetIter};

use crate::cfg::NUM_HART_MAX;

/// A validated RISC-V hart identifier.
///
/// The first implementation retains the firmware's existing contract that
/// `mhartid` is a direct index below [`NUM_HART_MAX`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HartId(usize);

/// Failure to turn a hardware hart identifier into a configured [`HartId`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HartIdError {
    /// The identifier is outside Runtime's configured capacity.
    OutOfRange,
}

impl HartId {
    /// Reads and validates the current hart's `mhartid`.
    #[inline]
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    pub fn current() -> Result<Self, HartIdError> {
        Self::from_raw(crate::csr::mhartid())
    }

    /// Validates a raw hardware hart identifier.
    #[inline]
    pub fn from_raw(raw: usize) -> Result<Self, HartIdError> {
        if raw < NUM_HART_MAX {
            Ok(Self(raw))
        } else {
            Err(HartIdError::OutOfRange)
        }
    }

    /// Returns the hardware identifier used by platform devices.
    #[inline]
    pub const fn as_usize(self) -> usize {
        self.0
    }
}
