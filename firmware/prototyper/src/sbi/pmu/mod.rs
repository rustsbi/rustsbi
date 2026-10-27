//! SBI PMU policy over Runtime's hardware counter controller.

#![forbid(unsafe_code)]

mod abi;
mod counters;
mod event;
mod firmware;

pub(crate) use firmware::FirmwareEvent;

use counters::CounterTable;
use event::HardwareEvents;
use runtime::{FdtNode, Result, hart::HartLocal, pmu::Pmu};
use spin::Mutex;

type PmuFactory = fn() -> Result<SbiPmu>;

/// Target-provided PMU fallback used when the device tree has no PMU node.
static DEFAULT: Mutex<Option<PmuFactory>> = Mutex::new(None);

pub(crate) fn set_default(factory: PmuFactory) {
    *DEFAULT.lock() = Some(factory);
}

pub(crate) fn take_default() -> Result<Option<SbiPmu>> {
    DEFAULT.lock().take().map(|factory| factory()).transpose()
}

/// The SBI PMU adapter owning event descriptions, logical records, and hardware control.
pub(crate) struct SbiPmu {
    pmu: Pmu,
    events: HardwareEvents,
    counters: HartLocal<Mutex<Option<CounterTable>>>,
}

impl SbiPmu {
    pub(crate) fn new() -> Result<Self> {
        let counters = HartLocal::new();
        counters.init_all(|_| Mutex::new(None))?;
        Ok(Self {
            pmu: Pmu::new(),
            events: HardwareEvents::default(),
            counters,
        })
    }

    pub(crate) fn from_node(node: FdtNode<'_, '_>) -> Result<Self> {
        Ok(Self {
            events: HardwareEvents::from_node(node)?,
            ..Self::new()?
        })
    }

    pub(crate) fn pmu(&self) -> &Pmu {
        &self.pmu
    }
}
