//! Machine timer and IPI drivers backed by one or more CLINTs.

mod kind;
mod routing;
mod sifive;
mod thead;

use alloc::boxed::Box;
use runtime::{
    Result,
    memory::{DeviceRegisterRange, MemoryRegistry},
};

pub(crate) use kind::ClintKind;
pub(crate) use routing::{RoutedIpi, RoutedTimer};

/// CLINT-local physical slots are not necessarily Runtime global hart IDs.
pub(super) trait LocalTimer: Send + Sync {
    fn set_compare_slot(&self, slot: usize, deadline: u64);
    fn read_time(&self) -> Option<u64> {
        None
    }
}

pub(super) trait LocalIpi: Send + Sync {
    fn send_slot(&self, slot: usize) -> Result<()>;
    fn clear_slot(&self, slot: usize) -> Result<()>;
}

/// Bind register windows using the physical slot count, including disabled CPUs.
pub(super) fn bind(
    registers: DeviceRegisterRange,
    kind: ClintKind,
    memory: &mut MemoryRegistry,
    slot_count: usize,
) -> Result<(Box<dyn LocalTimer>, Box<dyn LocalIpi>)> {
    match kind {
        ClintKind::SiFive => sifive::bind(registers, memory, slot_count),
        ClintKind::THead => thead::bind(registers, memory, slot_count),
    }
}
