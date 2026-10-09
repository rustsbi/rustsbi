//! Machine timer and IPI drivers backed by a CLINT.

mod kind;
mod sifive;
mod thead;

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use alloc::boxed::Box;
use core::mem::size_of;
use runtime::hart::HartId;
use runtime::ipi::{InterruptSource, IpiDevice, IpiError, SoftwareInterruptDevice};
use runtime::timer::TimerDevice;

pub(crate) use kind::ClintKind;

/// Binds the selected CLINT timer and IPI devices.
///
/// `hart_id_upper_bound` is the exclusive raw hart ID bound used to size
/// directly indexed register windows, including gaps between enabled IDs.
pub(crate) fn bind(
    registers: DeviceRegisterRange,
    kind: ClintKind,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> runtime::Result<(Box<dyn TimerDevice>, Box<dyn IpiDevice>)> {
    match kind {
        ClintKind::SiFive => sifive::bind(registers, memory, hart_id_upper_bound),
        ClintKind::THead => thead::bind(registers, memory, hart_id_upper_bound),
    }
}

/// MSIP register window and software-interrupt operations for both CLINT layouts.
struct Msip {
    registers: MmioRegion,
}

impl Msip {
    fn new(registers: MmioRegion) -> Self {
        Self { registers }
    }

    fn write(&self, hart: HartId, pending: bool) -> Result<(), IpiError> {
        let offset = hart
            .as_usize()
            .checked_mul(size_of::<u32>())
            .ok_or(IpiError)?;
        self.registers
            .write(offset, u32::from(pending))
            .map_err(|_| IpiError)
    }
}

impl IpiDevice for Msip {
    fn send(&self, hart: HartId) -> Result<(), IpiError> {
        self.write(hart, true)
    }

    fn interrupt_source(&self) -> InterruptSource<'_> {
        InterruptSource::Software(self)
    }
}

impl SoftwareInterruptDevice for Msip {
    fn clear(&self, hart: HartId) -> Result<(), IpiError> {
        self.write(hart, false)
    }
}
