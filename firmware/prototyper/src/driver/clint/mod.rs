//! Machine timer and IPI drivers backed by a CLINT.

mod kind;
mod sifive;
mod thead;

use runtime::{
    Result,
    ipi::{IpiDevice, Software},
    memory::{RegRange, MemoryRegistry},
    timer::TimerDevice,
};

use alloc::boxed::Box;

pub(crate) use kind::ClintKind;

/// Binds the selected CLINT timer and IPI devices.
///
/// `hart_id_upper_bound` is the exclusive raw hart ID bound used to size
/// directly indexed register windows, including gaps between enabled IDs.
pub(super) fn bind(
    registers: RegRange,
    kind: ClintKind,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> Result<(Box<dyn TimerDevice>, Box<dyn IpiDevice<Software>>)> {
    match kind {
        ClintKind::SiFive => sifive::bind(registers, memory, hart_id_upper_bound),
        ClintKind::THead => thead::bind(registers, memory, hart_id_upper_bound),
    }
}

/// Binds the CLINT timer independently of its software-interrupt device.
pub(super) fn bind_timer(
    registers: RegRange,
    kind: ClintKind,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> Result<Box<dyn TimerDevice>> {
    match kind {
        ClintKind::SiFive => sifive::bind_timer(registers, memory, hart_id_upper_bound),
        ClintKind::THead => thead::bind_timer(registers, memory, hart_id_upper_bound),
    }
}
