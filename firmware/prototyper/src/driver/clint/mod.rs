//! Machine timer and IPI drivers backed by a CLINT.

mod kind;
mod sifive;
mod thead;

use runtime::{
    Result,
    ipi::IpiDevice,
    memory::{DeviceRegisterRange, MemoryRegistry},
    timer::TimerDevice,
};

use alloc::boxed::Box;

pub(crate) use kind::ClintKind;

/// Binds the selected CLINT timer and IPI devices.
///
/// `hart_id_upper_bound` is the exclusive raw hart ID bound used to size
/// directly indexed register windows, including gaps between enabled IDs.
pub(super) fn bind(
    registers: DeviceRegisterRange,
    kind: ClintKind,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> Result<(Box<dyn TimerDevice>, Box<dyn IpiDevice>)> {
    match kind {
        ClintKind::SiFive => sifive::bind(registers, memory, hart_id_upper_bound),
        ClintKind::THead => thead::bind(registers, memory, hart_id_upper_bound),
    }
}
