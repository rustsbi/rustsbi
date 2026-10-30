//! RISC-V hart identity, sets, local storage, and lifecycle.
//!
//! SBI HSM policy and ABI translation belong to the firmware.

use crate::Result;

pub use csr::machine::TargetPrivilege;
mod descriptor;
mod extensions;
mod local;
mod set;
mod state;
mod state_enable;
mod topology;
mod wakeup;

pub use descriptor::{Hart, PrivilegedVersion};
pub use extensions::IsaExtensions;
pub use state_enable::StateEnable;

pub use wakeup::{HartWake, install_wakeup};

pub use local::HartLocal;
pub use set::{HartSet, HartSetIter};
pub(crate) use state::{ControlTransfer, HartEvent};
pub use state::{HartState, StartOutcome};

pub(crate) use topology::{HART_TABLE, HartEntry, publish};

#[inline]
pub(crate) fn current_hart() -> HartId {
    HartId::current().expect("BUG: current hart is not in the boot topology")
}

/// A validated RISC-V hart identifier.
///
/// Hardware IDs may be sparse.
/// Runtime assigns each enabled hart a compact index once during boot; devices
/// and SBI handoffs retain the hardware ID.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct HartId {
    raw: usize,
    index: usize,
}

impl HartId {
    /// Reads and validates the current hart's `mhartid`.
    #[inline]
    pub fn current() -> Result<Self> {
        use csr::machine;
        Self::from_raw(machine::HartId::read().into())
    }

    /// Validates a raw hardware hart identifier.
    #[inline]
    pub fn from_raw(raw: usize) -> Result<Self> {
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
