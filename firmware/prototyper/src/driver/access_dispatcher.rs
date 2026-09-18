//! Prototyper-side load/store access dispatcher.
//!
//! Connects Runtime's function-pointer access boundary to the concrete
//! platform peripherals that emulate faulting S-mode accesses.

#![forbid(unsafe_code)]

use spin::Once;

use super::spacemit_k1_syscon_apmu::SpacemitK1SysconApmu;

/// Routes an emulated access to the peripheral owning its physical address.
pub(crate) struct AccessDispatcher {
    pub(crate) spacemit_k1_syscon_apmu: Option<&'static SpacemitK1SysconApmu>,
}

impl AccessDispatcher {
    fn load(&self, addr: usize, width: usize) -> Option<usize> {
        if let Some(apmu) = self.spacemit_k1_syscon_apmu
            && let Some(value) = apmu.load(addr, width)
        {
            return Some(value);
        }
        None
    }

    fn store(&self, addr: usize, width: usize, value: usize) -> bool {
        if let Some(apmu) = self.spacemit_k1_syscon_apmu
            && apmu.store(addr, width, value)
        {
            return true;
        }
        false
    }
}

static DISPATCHER: Once<AccessDispatcher> = Once::new();

/// Publishes the access dispatcher once during boot.
pub(crate) fn install(dispatcher: AccessDispatcher) {
    DISPATCHER.call_once(|| dispatcher);
}

/// Runtime load callback: forwards to the published dispatcher.
pub(crate) fn load_handler(addr: usize, width: usize) -> Option<usize> {
    DISPATCHER.get()?.load(addr, width)
}

/// Runtime store callback: forwards to the published dispatcher.
pub(crate) fn store_handler(addr: usize, width: usize, value: usize) -> bool {
    DISPATCHER
        .get()
        .is_some_and(|dispatcher| dispatcher.store(addr, width, value))
}
