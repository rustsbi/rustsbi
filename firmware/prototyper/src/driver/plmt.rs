//! Andes PLMT: MTIME at +0, per-hart MTIMECMP at +8.

use crate::driver::TimerBackend;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    CounterLow = 0x00,
    CounterHigh = 0x04,
    CompareLow = 0x08,
    CompareHigh = 0x0c,
}

const COMPARE_STRIDE: usize = 8;

pub(super) struct Plmt {
    registers: MmioRegion,
    hart_id_upper_bound: usize,
}

/// Binds the timer registers below the exclusive raw hart ID upper bound.
pub(super) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> runtime::Result<Plmt> {
    if hart_id_upper_bound == 0 {
        return Err(runtime::Error::InvalidArgs);
    }
    let registers = registers.subrange(
        0,
        hart_id_upper_bound
            .checked_mul(COMPARE_STRIDE)
            .and_then(|size| size.checked_add(Register::CompareLow as usize))
            .ok_or(runtime::Error::Overflow)?,
    )?;
    if !registers.has_aligned_bounds(8) {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(Plmt {
        registers: memory.acquire_mmio(registers)?,
        hart_id_upper_bound,
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
            let high = self.read_word(Register::CounterHigh);
            let low = self.read_word(Register::CounterLow);
            if high == self.read_word(Register::CounterHigh) {
                return Some((u64::from(high) << 32) | u64::from(low));
            }
        }
    }

    fn read_time_low(&self) -> Option<usize> {
        #[cfg(target_pointer_width = "32")]
        {
            Some(self.read_word(Register::CounterLow) as usize)
        }
        #[cfg(target_pointer_width = "64")]
        {
            self.read_time().map(|value| value as usize)
        }
    }

    #[cfg(target_pointer_width = "32")]
    fn read_time_high(&self) -> Option<usize> {
        Some(self.read_word(Register::CounterHigh) as usize)
    }

    fn set_timer(&self, hart_id: usize, value: u64) {
        assert!(hart_id < self.hart_id_upper_bound);
        // Safe RV32 comparator update even if the old high half matches MTIME.
        self.write_compare_word(Register::CompareLow, hart_id, u32::MAX);
        self.write_compare_word(Register::CompareHigh, hart_id, (value >> 32) as u32);
        self.write_compare_word(Register::CompareLow, hart_id, value as u32);
        riscv::asm::fence();
    }

    // Masking MTIE prevents retrapping until the next set_timer rearms the comparator.
    fn clear_on_interrupt(&self) -> bool {
        false
    }
}
