//! Machine timer and IPI drivers backed by a CLINT.

mod kind;
mod sifive;
mod thead;

use runtime::memory::{DeviceRegisterRange, MemoryRegistry};

use alloc::boxed::Box;

use crate::driver::{IpiBackend, TimerBackend};

pub(crate) use kind::ClintKind;

/// Binds the selected CLINT timer and IPI devices.
pub(super) fn bind(
    registers: DeviceRegisterRange,
    kind: ClintKind,
    memory: &mut MemoryRegistry,
) -> runtime::Result<(Box<dyn TimerBackend>, Box<dyn IpiBackend + Send + Sync>)> {
    match kind {
        ClintKind::SiFive => sifive::bind(registers, memory),
        ClintKind::THead => thead::bind(registers, memory),
    }
}
