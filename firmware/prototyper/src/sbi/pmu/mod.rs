//! SBI PMU policy over Runtime's hardware counter controller.

#![forbid(unsafe_code)]

mod abi;
mod counters;
mod event;
mod firmware;

pub(crate) use firmware::{FirmwareEvent, from_exception};

use counters::CounterTable;
use event::HardwareEvents;
use runtime::{FdtNode, Result, hart::HartLocal};
use spin::Mutex;

/// SBI event descriptions, logical counter records, and software counts.
pub struct SbiPmu {
    events: HardwareEvents,
    counters: HartLocal<Mutex<Option<CounterTable>>>,
}

impl SbiPmu {
    pub(crate) fn new() -> Result<Self> {
        Self::with_events(HardwareEvents::default())
    }

    pub(crate) fn from_node(node: FdtNode<'_, '_>) -> Result<Self> {
        Self::with_events(HardwareEvents::from_node(node)?)
    }

    fn with_events(events: HardwareEvents) -> Result<Self> {
        let counters = HartLocal::new();
        counters.init_all(|_| Mutex::new(None))?;
        Ok(Self { events, counters })
    }
}
