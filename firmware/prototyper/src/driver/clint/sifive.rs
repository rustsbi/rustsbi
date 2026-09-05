//! SiFive Core Local Interruptor (CLINT).
//!
//! # References
//!
//! - Specification: [RISC-V ACLINT 1.0-rc4](https://github.com/riscvarchive/riscv-aclint/blob/4e570bfd3201f2c09e5afd290b5091526b0f099a/riscv-aclint.adoc) —
//!   “Backward Compatibility With SiFive CLINT” and its offset table.

use alloc::boxed::Box;
use core::mem::{align_of, size_of};

use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::cfg::NUM_HART_MAX;
use crate::driver::{InterruptDevices, IpiDevice, TimerDevice};

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
        assert!(
            hart_id < NUM_HART_MAX,
            "BUG: SiFive CLINT timer hart index is out of range"
        );
        Self::Mtimecmp.offset() + hart_id * size_of::<u64>()
    }
}

#[repr(usize)]
#[derive(Clone, Copy)]
enum IpiRegister {
    Msip = 0,
}

impl IpiRegister {
    const fn offset(self) -> usize {
        self as usize
    }

    fn offset_for_hart(self, hart_id: usize) -> usize {
        assert!(
            hart_id < NUM_HART_MAX,
            "BUG: SiFive CLINT IPI hart index is out of range"
        );
        self.offset() + hart_id * size_of::<u32>()
    }
}

pub(super) fn bind(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> runtime::Result<InterruptDevices> {
    let ipi_window_size = NUM_HART_MAX
        .checked_mul(size_of::<u32>())
        .ok_or(runtime::Error::Overflow)?;
    let timer_window_size = TimerRegister::Mtime
        .offset()
        .checked_add(size_of::<u64>())
        .ok_or(runtime::Error::Overflow)?;
    let ipi_registers = registers.subrange(0, ipi_window_size)?;
    let timer_registers = registers.subrange(MTIMECMP_OFFSET, timer_window_size)?;
    if !ipi_registers.has_aligned_bounds(align_of::<u32>())
        || !timer_registers.has_aligned_bounds(align_of::<u64>())
    {
        return Err(runtime::Error::InvalidArgs);
    }

    let ipi_mmio = memory.acquire_mmio(ipi_registers)?;
    let timer_mmio = memory.acquire_mmio(timer_registers)?;
    Ok(InterruptDevices {
        timer: Box::new(SiFiveTimer::new(timer_mmio)),
        ipi: Box::new(SiFiveIpi::new(ipi_mmio)),
    })
}

struct SiFiveTimer {
    registers: MmioRegion,
}

impl SiFiveTimer {
    fn new(registers: MmioRegion) -> Self {
        Self { registers }
    }

    fn read(&self, reg: TimerRegister) -> u64 {
        self.registers
            .read(reg.offset())
            .expect("BUG: SiFive CLINT timer register escaped its MMIO window")
    }

    fn write_mtimecmp(&self, hart_id: usize, value: u64) {
        self.registers
            .write(TimerRegister::mtimecmp_offset_for_hart(hart_id), value)
            .expect("BUG: SiFive CLINT timer register escaped its MMIO window")
    }
}

impl TimerDevice for SiFiveTimer {
    #[inline(always)]
    fn read_time(&self) -> u64 {
        self.read(TimerRegister::Mtime)
    }

    #[inline(always)]
    fn set_timer(&self, hart_id: usize, value: u64) {
        self.write_mtimecmp(hart_id, value)
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

    fn write(&self, reg: IpiRegister, hart_id: usize, value: IpiState) {
        self.registers
            .write(reg.offset_for_hart(hart_id), value as u32)
            .expect("BUG: SiFive CLINT IPI register escaped its MMIO window")
    }
}

impl IpiDevice for SiFiveIpi {
    #[inline(always)]
    fn send_ipi(&self, hart_id: usize) {
        self.write(IpiRegister::Msip, hart_id, IpiState::Pending)
    }

    #[inline(always)]
    fn clear_ipi(&self) {
        self.write(
            IpiRegister::Msip,
            crate::riscv::current_hartid(),
            IpiState::Clear,
        )
    }
}
