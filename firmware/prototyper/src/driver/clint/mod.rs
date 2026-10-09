//! Machine timer and IPI drivers backed by a CLINT.

mod kind;
mod sifive;
mod thead;

use runtime::memory::{DeviceRegisterRange, MemoryRegistry};

use alloc::boxed::Box;

use crate::driver::IpiBackend;
use runtime::timer::TimerDevice;

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
) -> runtime::Result<(Box<dyn TimerDevice>, Box<dyn IpiBackend + Send + Sync>)> {
    match kind {
        ClintKind::SiFive => sifive::bind(registers, memory, hart_id_upper_bound),
        ClintKind::THead => thead::bind(registers, memory, hart_id_upper_bound),
    }
}
