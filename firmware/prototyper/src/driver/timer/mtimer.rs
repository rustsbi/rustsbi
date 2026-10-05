//! ACLINT MTIMER: per-hart MTIMECMP at +8.
//!
//! Only `mtimecmp` is bound; time comes from the `time` CSR.

use crate::{cfg::NUM_HART_MAX, driver::TimerBackend};
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    CompareLow = 0x00,
    CompareHigh = 0x04,
}

const COMPARE_STRIDE: usize = 8;

pub(in crate::driver) struct Mtimer {
    registers: MmioRegion,
    hart_count: usize,
}

pub(in crate::driver) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_count: usize,
) -> runtime::Result<Mtimer> {
    if hart_count == 0 || hart_count > NUM_HART_MAX {
        return Err(runtime::Error::InvalidArgs);
    }
    let window = Register::CompareLow as usize + COMPARE_STRIDE * hart_count;
    let registers = registers.subrange(0, window)?;
    if !registers.has_aligned_bounds(COMPARE_STRIDE) {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(Mtimer {
        registers: memory.acquire_mmio(registers)?,
        hart_count,
    })
}

impl Mtimer {
    #[inline(always)]
    fn write_compare_word(&self, register: Register, hart_id: usize, value: u32) {
        self.registers
            .write(register as usize + COMPARE_STRIDE * hart_id, value.to_le())
            .expect("ACLINT MTIMER write outside acquired window");
    }
}

impl TimerBackend for Mtimer {
    fn set_timer(&self, hart_id: usize, value: u64) {
        assert!(hart_id < self.hart_count);
        // Safe RV32 comparator update even if the old high half matches MTIME.
        self.write_compare_word(Register::CompareLow, hart_id, u32::MAX);
        self.write_compare_word(Register::CompareHigh, hart_id, (value >> 32) as u32);
        self.write_compare_word(Register::CompareLow, hart_id, value as u32);
        riscv::asm::fence();
    }
}
