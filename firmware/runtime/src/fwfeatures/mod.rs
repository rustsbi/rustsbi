//! Hart-local supervisor feature configuration.
//!
//! Runtime owns hardware access and restores committed values before handing
//! off a fresh supervisor context. SBI policy owns feature locks.

mod access;
mod feature;
mod hardware;
mod state;

use crate::{hart::HartLocal, irq::LocalIrqDisabled};
use spin::Mutex;
use state::State;

pub use csr::machine::{FeatureState, PointerMasking};
pub use feature::{Feature, Value};

/// Runtime-owned firmware feature configuration and hardware access.
/// Runtime initializes each hart before lower-mode execution; callers access
/// the current hart through [`Self::with_current`].
/// Committed values survive HSM stop/start and suspend/resume.
pub struct FwFeatures {
    /// Committed feature values for each hart, used to restore its hardware configuration.
    local: HartLocal<Mutex<State>>,
}

static FWFEATURES: FwFeatures = FwFeatures {
    local: HartLocal::new(),
};

/// Current-hart feature access during one transaction with local IRQs disabled.
/// The view cannot outlive the transaction or move to another hart.
pub struct Features<'scope> {
    state: &'scope mut State,
    _irq: &'scope LocalIrqDisabled,
}
