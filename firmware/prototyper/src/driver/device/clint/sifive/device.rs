//! SiFive Core Local Interruptor (CLINT).
//!
//! # References
//!
//! - Specification: [RISC-V ACLINT 1.0-rc4](https://github.com/riscvarchive/riscv-aclint/blob/4e570bfd3201f2c09e5afd290b5091526b0f099a/riscv-aclint.adoc) —
//!   “Backward Compatibility With SiFive CLINT” and its offset table.

use core::mem::{align_of, size_of};
use runtime::{
    Error, Result,
    hart::HartId,
    memory::{MemoryRegistry, MmioRegion},
    timer::TimerDevice,
};

use super::description::Desc;

// The ACLINT legacy mapping places MTIMECMP at 0x4000 and MTIME at 0xbff8.
const MTIMECMP_OFFSET: usize = 0x4000;
const MTIME_WINDOW_OFFSET: usize = 0xbff8 - MTIMECMP_OFFSET;

/// SiFive CLINT timer-register offsets.
#[repr(usize)]
#[derive(Clone, Copy)]
enum TimerRegister {
    Mtimecmp = 0,
    Mtime = MTIME_WINDOW_OFFSET,
}

impl TimerRegister {
    const fn offset(self) -> usize {
        self as usize
    }

    fn mtimecmp_offset_for_hart(hart: HartId) -> usize {
        Self::Mtimecmp.offset() + hart.as_usize() * size_of::<u64>()
    }
}

/// A SiFive CLINT machine timer.
pub(super) struct SiFiveTimer {
    registers: MmioRegion,
}

impl SiFiveTimer {
    /// Acquires the legacy `mtimecmp` and `mtime` window.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let mtimecmp_size = HartId::upper_bound()?
            .checked_mul(size_of::<u64>())
            .ok_or(Error::Overflow)?;
        if mtimecmp_size > MTIME_WINDOW_OFFSET {
            return Err(Error::InvalidArgs);
        }
        let window_size = TimerRegister::Mtime
            .offset()
            .checked_add(size_of::<u64>())
            .ok_or(Error::Overflow)?;
        let registers = description
            .registers
            .subrange(MTIMECMP_OFFSET, window_size)?;
        if !registers.has_aligned_bounds(align_of::<u64>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire(registers)?,
        })
    }

    /// Reads a 64-bit timer register, retrying the high word on RV32.
    fn read(&self, reg: TimerRegister) -> u64 {
        #[cfg(target_pointer_width = "64")]
        return self
            .registers
            .read(reg.offset())
            .expect("BUG: SiFive CLINT timer register escaped its MMIO window");

        #[cfg(target_pointer_width = "32")]
        loop {
            let high = self.read_word(reg.offset() + 4);
            let low = self.read_word(reg.offset());
            // Re-read the high word to detect a carry between the two halves.
            if high == self.read_word(reg.offset() + 4) {
                return (u64::from(high) << 32) | u64::from(low);
            }
        }
    }

    #[cfg(target_pointer_width = "32")]
    fn read_word(&self, offset: usize) -> u32 {
        self.registers
            .read(offset)
            .expect("BUG: SiFive CLINT timer register escaped its MMIO window")
    }

    #[cfg(target_pointer_width = "32")]
    fn write_word(&self, offset: usize, value: u32) {
        self.registers
            .write(offset, value)
            .expect("BUG: SiFive CLINT timer register escaped its MMIO window")
    }
}

impl TimerDevice for SiFiveTimer {
    #[inline(always)]
    fn set_compare(&self, hart: HartId, value: u64) {
        let offset = TimerRegister::mtimecmp_offset_for_hart(hart);
        #[cfg(target_pointer_width = "64")]
        self.registers
            .write(offset, value)
            .expect("BUG: SiFive CLINT timer register escaped its MMIO window");

        #[cfg(target_pointer_width = "32")]
        {
            // Raise the low word first to prevent an early interrupt while the
            // high and low halves of the new compare value are installed.
            self.write_word(offset, u32::MAX);
            self.write_word(offset + 4, (value >> 32) as u32);
            self.write_word(offset, value as u32);
        }
    }

    #[inline(always)]
    fn read_time(&self) -> Option<u64> {
        Some(self.read(TimerRegister::Mtime))
    }
}
