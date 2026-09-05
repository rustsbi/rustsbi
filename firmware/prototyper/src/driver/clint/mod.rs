//! Machine timer and IPI drivers backed by a CLINT.

mod kind;
mod sifive;
mod thead;

use runtime::memory::{DeviceRegisterRange, MemoryRegistry};

use crate::driver::InterruptDevices;

pub(crate) use kind::ClintKind;

/// Binds the selected CLINT timer and IPI devices.
pub(super) fn bind(
    registers: DeviceRegisterRange,
    kind: ClintKind,
    memory: &mut MemoryRegistry,
) -> runtime::Result<InterruptDevices> {
    match kind {
        ClintKind::SiFive => sifive::bind(registers, memory),
        ClintKind::THead => thead::bind(registers, memory),
    }
}
