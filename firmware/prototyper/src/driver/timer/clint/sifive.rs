//! SiFive-layout Core Local Interruptor timer.
//!
//! # References
//!
//! - Specification: [RISC-V ACLINT 1.0-rc4](https://github.com/riscvarchive/riscv-aclint/blob/4e570bfd3201f2c09e5afd290b5091526b0f099a/riscv-aclint.adoc) —
//!   “Backward Compatibility With SiFive CLINT” and its offset table.

use alloc::boxed::Box;
use core::mem::{align_of, size_of};
use runtime::{
    Error, Result,
    hart::HartId,
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
    timer::TimerDevice,
};

use crate::devicetree::EnabledNode;
use crate::driver::clint;
use crate::driver::timer::{Description, Driver, Timer, hart_id_upper_bound};

/// Timer name reported in boot logs.
const NAME: &str = "SiFiveClint";

/// SiFive-layout timer driver.
pub(crate) struct SiFiveTimerDriver;

pub(crate) static SIFIVE: SiFiveTimerDriver = SiFiveTimerDriver;

impl Driver<Timer> for SiFiveTimerDriver {
    fn compatibles(&self) -> &'static [&'static str] {
        &clint::SIFIVE_COMPATIBLES
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a runtime::PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        let registers = platform
            .device_register(device.node())?
            .ok_or(Error::InvalidArgs)?;
        let hart_id_upper_bound = hart_id_upper_bound()?;
        Ok(Some(Description::new(NAME, registers, move |memory| {
            bind(registers, memory, hart_id_upper_bound)
        })))
    }
}

// The ACLINT legacy mapping places MTIME at 0xbff8.
const MTIME_WINDOW_OFFSET: usize = 0xbff8 - clint::MTIMECMP_OFFSET;

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

fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
    hart_id_upper_bound: usize,
) -> Result<Box<dyn TimerDevice>> {
    let mtimecmp_size_bytes = hart_id_upper_bound
        .checked_mul(size_of::<u64>())
        .ok_or(Error::Overflow)?;
    if mtimecmp_size_bytes > MTIME_WINDOW_OFFSET {
        return Err(Error::InvalidArgs);
    }
    let timer_window_size_bytes = TimerRegister::Mtime
        .offset()
        .checked_add(size_of::<u64>())
        .ok_or(Error::Overflow)?;
    let timer_registers = registers.subrange(clint::MTIMECMP_OFFSET, timer_window_size_bytes)?;
    if !timer_registers.has_aligned_bounds(align_of::<u64>()) {
        return Err(Error::InvalidArgs);
    }
    let timer_mmio = memory.acquire(timer_registers)?;
    Ok(Box::new(SiFiveTimer::new(timer_mmio)))
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
        let offset = TimerRegister::mtimecmp_offset_for_hart(hart.as_usize());
        #[cfg(target_pointer_width = "64")]
        self.registers
            .write(offset, value)
            .expect("BUG: SiFive CLINT timer register escaped its MMIO window");

        #[cfg(target_pointer_width = "32")]
        {
            // Compare-safe split writes, as in the T-Head CLINT backend.
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
