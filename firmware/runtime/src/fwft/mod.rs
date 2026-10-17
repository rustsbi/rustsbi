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

pub use csr::machine::PointerMasking;
pub use feature::{Feature, Value};

/// Hardware feature configuration owned by the published SBI adapter.
/// Runtime initializes the registered instance once per hart before lower-mode
/// execution.
/// Committed values survive HSM stop/start and suspend/resume.
pub struct Fwft {
    local: HartLocal<Mutex<State>>,
}

/// Current-hart feature access during one transaction with local IRQs disabled.
/// The view cannot outlive the transaction or move to another hart.
pub struct Features<'scope> {
    state: &'scope mut State,
    _irq: &'scope LocalIrqDisabled,
}

impl Default for Fwft {
    fn default() -> Self {
        Self::new()
    }
}

impl Fwft {
    /// Creates an unbound feature controller without accessing hardware.
    pub const fn new() -> Self {
        Self {
            local: HartLocal::new(),
        }
    }
}
