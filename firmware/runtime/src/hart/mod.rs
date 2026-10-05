//! RISC-V hart identity, sets, local storage, and lifecycle.
//!
//! SBI HSM policy and ABI translation belong to the firmware.

mod lifecycle;
mod local;
mod set;
mod topology;
mod wakeup;

pub use wakeup::{HartWake, install_wakeup};

pub(crate) use lifecycle::{
    ControlTransfer, HartEvent, current_hart, take_control_transfer, take_local_event,
};
pub use lifecycle::{
    HartState, ResumeError, ResumeTicket, StageError, StartError, StartOutcome, StopError,
    SuspendError, begin_nonretentive_resume, can_receive_ipi, resume_current_retentive,
    stage_current, start, status, stop_current, suspend_current,
};
pub use local::{HartLocal, HartLocalError};
pub use set::{HartSet, HartSetIter};

pub(crate) use topology::{HART_TABLE, HartEntry, publish};

/// A validated RISC-V hart identifier.
///
/// Hardware IDs may be sparse. Runtime assigns each enabled hart a compact
/// index once during boot; devices and SBI handoffs retain the hardware ID.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct HartId {
    raw: usize,
    index: usize,
}

/// Failure to turn a hardware hart identifier into a configured [`HartId`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HartIdError {
    /// Boot has not published the hart topology yet.
    Unavailable,
    /// The identifier does not belong to an enabled hart.
    Unknown,
}

impl HartId {
    /// Reads and validates the current hart's `mhartid`.
    #[inline]
    pub fn current() -> Result<Self, HartIdError> {
        match () {
            #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
            () => Self::from_raw(crate::csr::mhartid()),
            #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
            () => unimplemented!("Reading the current hart ID requires a RISC-V target"),
        }
    }

    /// Validates a raw hardware hart identifier.
    #[inline]
    pub fn from_raw(raw: usize) -> Result<Self, HartIdError> {
        topology::from_raw(raw)
    }

    /// Returns the hardware identifier used by platform devices.
    #[inline]
    pub const fn as_usize(self) -> usize {
        self.raw
    }

    /// Returns the compact index used by per-hart software storage.
    #[inline]
    pub const fn index(self) -> usize {
        self.index
    }

    /// Returns the number of enabled harts, or zero before boot publication.
    pub fn count() -> usize {
        topology::count()
    }

    /// Iterates over enabled harts in ascending hardware-ID order.
    pub fn all() -> impl DoubleEndedIterator<Item = Self> + ExactSizeIterator {
        topology::all()
    }
}
