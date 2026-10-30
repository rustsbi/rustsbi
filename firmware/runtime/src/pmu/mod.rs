//! Runtime-owned hardware performance-counter access.
//!
//! SBI policy owns event mappings, logical counter indices, and software counts.
//! Runtime initializes each hart's counter access and excludes local interrupts
//! across each scoped counter transaction.

mod access;
mod hardware;

use crate::{Error, Result, hart::HartLocal, irq::LocalIrqDisabled};
pub use csr::values::counter::{HardwareCounter, HpmCounter};
use spin::Mutex;

pub(crate) use access::configure_initial;

/// Runtime-owned hardware counter control.
pub struct Pmu {
    local: HartLocal<Mutex<Counter>>,
    hardware: Mutex<()>,
}

struct Counter {
    widths: [u8; 32],
    inhibit: InhibitSupport,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) enum InhibitSupport {
    Unavailable,
    Available,
}

/// Access to this hart's counters during one IRQ-excluded transaction.
/// The view cannot outlive the transaction or move to another hart.
pub struct Counters<'scope> {
    state: &'scope Counter,
    _irq: &'scope LocalIrqDisabled,
}

static PMU: Pmu = Pmu {
    local: HartLocal::new(),
    hardware: Mutex::new(()),
};

impl Pmu {
    /// Returns whether Runtime initialized this hart's hardware binding.
    pub fn is_available() -> bool {
        PMU.local.try_get_current().is_some()
    }
}

impl Counters<'_> {
    /// Returns implemented architectural counter positions, including time.
    pub fn implemented(&self) -> u32 {
        self.state
            .widths
            .iter()
            .enumerate()
            .fold(0, |bits, (index, width)| {
                bits | (u32::from(*width != 0) << index)
            })
    }

    /// Returns the discovered counter width in bits.
    pub fn width(&self, counter: HardwareCounter) -> Result<u8> {
        let width = self.state.widths[counter.index() as usize];
        if width == 0 {
            Err(Error::NotSupported)
        } else {
            Ok(width)
        }
    }
}
