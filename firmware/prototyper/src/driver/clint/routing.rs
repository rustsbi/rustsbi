//! Routes global hart IDs to each CLINT's physical register slots.

use alloc::{boxed::Box, vec::Vec};
use runtime::{
    Result,
    hart::HartId,
    ipi::{IpiDevice, IpiRequest},
    timer::TimerDevice,
};

use super::{LocalIpi, LocalTimer};

fn target(routes: &[(usize, usize)], hart: HartId) -> (usize, usize) {
    // Device binding rejects any missing, duplicate or out-of-range route.
    routes[hart.index()]
}

/// Owns per-controller timer resources and dispatches per-hart compare writes.
pub(crate) struct RoutedTimer {
    backends: Vec<Box<dyn LocalTimer>>,
    routes: Vec<(usize, usize)>,
}

impl RoutedTimer {
    pub(crate) fn new(backends: Vec<Box<dyn LocalTimer>>, routes: Vec<(usize, usize)>) -> Self {
        Self { backends, routes }
    }
}

impl TimerDevice for RoutedTimer {
    fn set_compare(&self, hart: HartId, deadline: u64) {
        let (device, slot) = target(&self.routes, hart);
        self.backends[device].set_compare_slot(slot, deadline);
    }

    fn read_time(&self) -> Option<u64> {
        if self.backends.len() == 1 {
            return self.backends[0].read_time();
        }
        // Independent counters may differ: use the current hart's controller.
        let hart = HartId::current().ok()?;
        let (device, _) = target(&self.routes, hart);
        self.backends[device].read_time()
    }
}

/// Owns controller-local MSIP resources for all global SBI IPI targets.
pub(crate) struct RoutedIpi {
    backends: Vec<Box<dyn LocalIpi>>,
    routes: Vec<(usize, usize)>,
}

impl RoutedIpi {
    pub(crate) fn new(backends: Vec<Box<dyn LocalIpi>>, routes: Vec<(usize, usize)>) -> Self {
        Self { backends, routes }
    }
}

impl IpiDevice for RoutedIpi {
    fn send(&self, request: IpiRequest) -> Result<()> {
        for raw_hart in request.harts() {
            let hart = HartId::from_raw(raw_hart)?;
            let (device, slot) = target(&self.routes, hart);
            self.backends[device].send_slot(slot)?;
        }
        Ok(())
    }

    fn acknowledge(&self) -> Result<bool> {
        let hart = HartId::current()?;
        let (device, slot) = target(&self.routes, hart);
        self.backends[device].clear_slot(slot)?;
        Ok(true)
    }
}
