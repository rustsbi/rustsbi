//! SiFive Core Local Interruptor (CLINT).
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
    ipi::{IpiDevice, IpiRequest},
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
    timer::TimerDevice,
};

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
) -> Result<(Box<dyn TimerDevice>, Box<dyn IpiDevice>)> {
    let msip_size_bytes = hart_id_upper_bound
        .checked_mul(size_of::<u32>())
        .ok_or(Error::Overflow)?;
    let mtimecmp_size_bytes = hart_id_upper_bound
        .checked_mul(size_of::<u64>())
        .ok_or(Error::Overflow)?;
    if msip_size_bytes > MTIMECMP_OFFSET || mtimecmp_size_bytes > MTIME_WINDOW_OFFSET {
        return Err(Error::InvalidArgs);
    }
    let timer_window_size_bytes = TimerRegister::Mtime
        .offset()
        .checked_add(size_of::<u64>())
        .ok_or(Error::Overflow)?;
    let ipi_registers = registers.subrange(0, msip_size_bytes)?;
    let timer_registers = registers.subrange(MTIMECMP_OFFSET, timer_window_size_bytes)?;
    if !ipi_registers.has_aligned_bounds(align_of::<u32>())
        || !timer_registers.has_aligned_bounds(align_of::<u64>())
    {
        return Err(Error::InvalidArgs);
    }

    let ipi_mmio = memory.acquire(ipi_registers)?;
    let timer_mmio = memory.acquire(timer_registers)?;
    Ok((
        Box::new(SiFiveTimer::new(timer_mmio)),
        Box::new(SiFiveIpi::new(ipi_mmio)),
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

#[repr(u32)]
enum IpiState {
    Clear = 0,
    Pending = 1,
}

struct SiFiveIpi {
    registers: MmioRegion,
}

impl SiFiveIpi {
    fn new(registers: MmioRegion) -> Self {
        Self { registers }
    }

    fn write_msip(&self, hart_id: usize, value: IpiState) -> Result<()> {
        let offset = hart_id
            .checked_mul(size_of::<u32>())
            .ok_or(Error::IoError)?;
        self.registers
            .write(offset, value as u32)
            .map_err(|_| Error::IoError)
    }
}

impl IpiDevice for SiFiveIpi {
    #[inline(always)]
    fn send(&self, req: IpiRequest) -> Result<()> {
        for hart_id in req.target_hart_ids() {
            self.write_msip(hart_id, IpiState::Pending)?;
        }
        Ok(())
    }

    #[inline(always)]
    fn acknowledge(&self) -> Result<bool> {
        let hart = HartId::current()?;
        self.write_msip(hart.as_usize(), IpiState::Clear)?;
        Ok(true)
    }
}
