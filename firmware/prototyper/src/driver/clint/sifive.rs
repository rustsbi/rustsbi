//! SiFive Core Local Interruptor (CLINT).
//!
//! # References
//!
//! - Specification: [RISC-V ACLINT 1.0-rc4](https://github.com/riscvarchive/riscv-aclint/blob/4e570bfd3201f2c09e5afd290b5091526b0f099a/riscv-aclint.adoc) —
//!   “Backward Compatibility With SiFive CLINT” and its offset table.

use alloc::boxed::Box;
use core::mem::{align_of, size_of};

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use runtime::hart::HartId;
use runtime::ipi::IpiDevice;
use runtime::timer::{TimeSource, TimerDevice};

use super::Msip;

// The ACLINT legacy mapping places MTIMECMP at 0x4000 and MTIME at 0xbff8.
const MTIMECMP_OFFSET: usize = 0x4000;
const MTIME_WINDOW_OFFSET: usize = 0xbff8 - MTIMECMP_OFFSET;

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

    fn mtimecmp_offset_for_hart(hart_id: usize) -> usize {
        Self::Mtimecmp.offset()
            + hart_id
                .checked_mul(size_of::<u64>())
                .expect("CLINT hart offset overflowed")
    }
}

pub(super) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> runtime::Result<(Box<dyn TimerDevice>, Box<dyn IpiDevice>)> {
    let msip_size_bytes = hart_id_upper_bound
        .checked_mul(size_of::<u32>())
        .ok_or(runtime::Error::Overflow)?;
    let mtimecmp_size_bytes = hart_id_upper_bound
        .checked_mul(size_of::<u64>())
        .ok_or(runtime::Error::Overflow)?;
    if msip_size_bytes > MTIMECMP_OFFSET || mtimecmp_size_bytes > MTIME_WINDOW_OFFSET {
        return Err(runtime::Error::InvalidArgs);
    }
    let timer_window_size_bytes = TimerRegister::Mtime
        .offset()
        .checked_add(size_of::<u64>())
        .ok_or(runtime::Error::Overflow)?;
    let ipi_registers = registers.subrange(0, msip_size_bytes)?;
    let timer_registers = registers.subrange(MTIMECMP_OFFSET, timer_window_size_bytes)?;
    if !ipi_registers.has_aligned_bounds(align_of::<u32>())
        || !timer_registers.has_aligned_bounds(align_of::<u64>())
    {
        return Err(runtime::Error::InvalidArgs);
    }

    let ipi_mmio = memory.acquire_mmio(ipi_registers)?;
    let timer_mmio = memory.acquire_mmio(timer_registers)?;
    Ok((
        Box::new(SiFiveTimer::new(timer_mmio)),
        Box::new(Msip::new(ipi_mmio)),
    ))
}

struct SiFiveTimer {
    registers: MmioRegion,
}

impl SiFiveTimer {
    fn new(registers: MmioRegion) -> Self {
        Self { registers }
    }

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
            if high == self.read_word(reg.offset() + 4) {
                return (u64::from(high) << 32) | u64::from(low);
            }
        }
    }

    fn write_mtimecmp(&self, hart_id: usize, value: u64) {
        let offset = TimerRegister::mtimecmp_offset_for_hart(hart_id);
        #[cfg(target_pointer_width = "64")]
        self.registers
            .write(offset, value)
            .expect("BUG: SiFive CLINT timer register escaped its MMIO window");

        #[cfg(target_pointer_width = "32")]
        {
            // Raise the low word first so a split update cannot assert an early interrupt.
            self.write_word(offset, u32::MAX);
            self.write_word(offset + 4, (value >> 32) as u32);
            self.write_word(offset, value as u32);
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
    fn set_deadline(&self, hart: HartId, deadline: u64) {
        self.write_mtimecmp(hart.as_usize(), deadline)
    }

    fn time_source(&self) -> Option<&dyn TimeSource> {
        Some(self)
    }
}

impl TimeSource for SiFiveTimer {
    #[inline(always)]
    fn read_time(&self) -> u64 {
        self.read(TimerRegister::Mtime)
    }
}
