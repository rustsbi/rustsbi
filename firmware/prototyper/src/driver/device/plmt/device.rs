//! Andes PLMT register operations.

use runtime::{
    Error, Result,
    hart::HartId,
    memory::{MemoryRegistry, MmioRegion},
    timer::TimerDevice,
};

use super::description::Desc;

/// PLMT register offsets.
#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    CounterLow = 0x00,
    CounterHigh = 0x04,
    CompareLow = 0x08,
    CompareHigh = 0x0c,
}

// Each hart's 64-bit compare value spans two 32-bit words.
const COMPARE_STRIDE: usize = 8;

/// An Andes PLMT machine timer.
pub(super) struct Plmt {
    registers: MmioRegion,
}

impl Plmt {
    /// Acquires the counter and comparator windows for enabled harts.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let registers = description.registers.subrange(
            0,
            HartId::upper_bound()?
                .checked_mul(COMPARE_STRIDE)
                .and_then(|size| size.checked_add(Register::CompareLow as usize))
                .ok_or(Error::Overflow)?,
        )?;
        if !registers.has_aligned_bounds(8) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire(registers)?,
        })
    }

    #[inline(always)]
    fn read_word(&self, register: Register) -> Result<u32> {
        self.registers
            .read::<u32>(register as usize)
            .map(u32::from_le)
    }

    #[inline(always)]
    fn write_compare_word(&self, register: Register, hart: HartId, value: u32) -> Result<()> {
        // Initialization checked the window through the highest enabled hart.
        let offset = hart.as_usize() * COMPARE_STRIDE + register as usize;
        self.registers.write(offset, value.to_le())
    }
}

impl TimerDevice for Plmt {
    /// Reads `MTIME` with a consistent high/low snapshot.
    fn read_time(&self) -> Result<Option<u64>> {
        loop {
            let high = self.read_word(Register::CounterHigh)?;
            let low = self.read_word(Register::CounterLow)?;
            // Re-read the high word to detect a carry between the two halves.
            if high == self.read_word(Register::CounterHigh)? {
                return Ok(Some((u64::from(high) << 32) | u64::from(low)));
            }
        }
    }

    fn set_compare(&self, hart: HartId, value: u64) -> Result<()> {
        // Raise the low word first to prevent an early interrupt while the
        // high and low halves of the new compare value are installed.
        self.write_compare_word(Register::CompareLow, hart, u32::MAX)?;
        self.write_compare_word(Register::CompareHigh, hart, (value >> 32) as u32)?;
        self.write_compare_word(Register::CompareLow, hart, value as u32)?;
        runtime::instruction::fence();
        Ok(())
    }
}
