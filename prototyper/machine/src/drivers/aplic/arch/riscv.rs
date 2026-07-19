//! MMIO transport for a validated RISC-V machine APLIC.

use super::super::{AplicError, Registers, configure};

struct Mmio {
    base: usize,
}

impl Registers for Mmio {
    fn read(&mut self, offset: usize) -> u32 {
        // SAFETY: validation proves the offset lies in the claimed range.
        unsafe { ((self.base + offset) as *const u32).read_volatile() }
    }

    fn write(&mut self, offset: usize, value: u32) {
        // SAFETY: same exclusive range proof as `read`.
        unsafe { ((self.base + offset) as *mut u32).write_volatile(value) }
    }

    fn fence(&mut self) {
        // SAFETY: the full device fence carries no memory operand.
        unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) }
    }
}

pub(in crate::drivers::aplic) fn configure_device(
    base: usize,
    source_count: u32,
    machine_imsic_base: u64,
    supervisor_imsic_base: u64,
    hart_index_bits: u32,
) -> Result<(), AplicError> {
    configure(
        &mut Mmio { base },
        source_count,
        machine_imsic_base,
        supervisor_imsic_base,
        hart_index_bits,
    )
}
