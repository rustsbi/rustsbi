//! Andes PLMT: MTIME at +0, per-hart MTIMECMP at +8.

use crate::{cfg::NUM_HART_MAX, driver::TimerBackend};
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    TimeLow = 0x00,
    TimeHigh = 0x04,
    TimeCompareLow = 0x08,
    TimeCompareHigh = 0x0c,
}

const COMPARE_STRIDE: usize = 8;

pub(super) struct Plmt {
    registers: MmioRegion,
    hart_count: usize,
}

pub(super) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_count: usize,
) -> runtime::Result<Plmt> {
    if hart_count == 0 || hart_count > NUM_HART_MAX {
        return Err(runtime::Error::InvalidArgs);
    }
    let registers = registers.subrange(
        0,
        Register::TimeCompareLow as usize + COMPARE_STRIDE * hart_count,
    )?;
    if !registers.has_aligned_bounds(8) {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(Plmt {
        registers: memory.acquire_mmio(registers)?,
        hart_count,
    })
}

impl Plmt {
    #[inline(always)]
    fn read_word(&self, register: Register) -> u32 {
        match self.registers.read::<u32>(register as usize) {
            Ok(value) => u32::from_le(value),
            Err(error) => panic!("PLMT read outside acquired window: {error:?}"),
        }
    }

    #[inline(always)]
    fn write_compare_word(&self, register: Register, hart_id: usize, value: u32) {
        self.registers
            .write(register as usize + COMPARE_STRIDE * hart_id, value.to_le())
            .expect("PLMT write outside acquired window");
    }
}

impl TimerBackend for Plmt {
    fn read_time(&self) -> Option<u64> {
        loop {
            let high = self.read_word(Register::TimeHigh);
            let low = self.read_word(Register::TimeLow);
            if high == self.read_word(Register::TimeHigh) {
                return Some((u64::from(high) << 32) | u64::from(low));
            }
        }
    }

    fn read_time_low(&self) -> Option<usize> {
        #[cfg(target_pointer_width = "32")]
        {
            Some(self.read_word(Register::TimeLow) as usize)
        }
        #[cfg(target_pointer_width = "64")]
        {
            self.read_time().map(|value| value as usize)
        }
    }

    #[cfg(target_pointer_width = "32")]
    fn read_time_high(&self) -> Option<usize> {
        Some(self.read_word(Register::TimeHigh) as usize)
    }

    fn set_timer(&self, hart_id: usize, value: u64) {
        assert!(hart_id < self.hart_count);
        // Safe RV32 comparator update even if the old high half matches MTIME.
        self.write_compare_word(Register::TimeCompareLow, hart_id, u32::MAX);
        self.write_compare_word(Register::TimeCompareHigh, hart_id, (value >> 32) as u32);
        self.write_compare_word(Register::TimeCompareLow, hart_id, value as u32);
        riscv::asm::fence();
    }

    // Masking MTIE prevents retrapping until the next set_timer rearms the comparator.
    fn clear_on_interrupt(&self) -> bool {
        false
    }
}
