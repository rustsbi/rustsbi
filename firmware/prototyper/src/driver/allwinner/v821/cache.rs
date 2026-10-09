//! A27L2 cache maintenance through V821 MMIO and Andes CSRs.
//!
//! This device is consumed by the V821 vendor SBI extension. Binding requires
//! the prepared V821 SoC and its discovered L2 register range.
//!
//! # References
//!
//! - Vendor implementation: [V821 SPL cache support](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/spl/arch/riscv/cpu/ads_rv32/mmu.c)
//!   — A27L2 MMIO layout, command encodings, cache-line size, and initialization
//!   bits.

use runtime::memory::{
    DeviceRegisterRange, MemoryRegistry, MmioRegion, PhysAddr, PhysAddrRange, SupervisorMemory,
};
use runtime::soc::allwinner::v821::{
    A27L2LineOperation as L1Operation, AllwinnerV821Soc, AndesStatusRegister,
};
use runtime::timer::TimeSource;

/// The A27L2 cache-maintenance MMIO device.
///
/// V821 preparation requires exactly one enabled hart, so its CSR/MMIO
/// command sequence needs no inter-hart mutex.
pub(crate) struct A27L2Cache {
    soc: AllwinnerV821Soc,
    registers: MmioRegion,
}

#[derive(Clone, Copy)]
#[repr(u32)]
enum L2Command {
    InvalidateRange = 8,
    WriteBackRange = 9,
    WriteBackAndInvalidateRange = 10,
    WriteBackAndInvalidateAll = 0x12,
}

#[derive(Clone, Copy)]
#[repr(usize)]
enum CacheRegister {
    Control = 0x08,
    Command = 0x40,
    LineAddress = 0x48,
    Status = 0x80,
}

/// BSP-required A27L2 control bits set during firmware binding.
const L2_CONTROL_INIT_MASK: u32 = (1 << 13) | (1 << 10) | 1;
const CACHE_LINE_SIZE: usize = 64;
const LARGE_FLUSH_THRESHOLD: usize = 128 * 1024;
const COMMAND_TIMEOUT_TICKS: u32 = 4_000_000;
const STATUS_STATE_MASK: u32 = 0xf;
const STATUS_BUSY: u32 = 1;

impl A27L2Cache {
    pub(crate) fn bind(
        soc: AllwinnerV821Soc,
        registers: DeviceRegisterRange,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        // Keep the entire described window reserved even though layout checks only
        // cover the registers this driver accesses.
        if !registers
            .subrange(0, CacheRegister::Status as usize + size_of::<u32>())?
            .has_aligned_bounds(align_of::<u32>())
        {
            return Err(runtime::Error::InvalidArgs);
        }
        let cache = Self {
            soc,
            registers: memory.acquire_mmio(registers)?,
        };
        let control = cache.read_u32(CacheRegister::Control)?;
        cache.write_u32(CacheRegister::Control, control | L2_CONTROL_INIT_MASK)?;
        cache.registers.synchronize();
        Ok(cache)
    }

    /// Reads an Andes machine-control status register.
    pub(crate) fn read_status(&self, register: AndesStatusRegister) -> usize {
        self.soc.read_status(register)
    }

    /// Writes back and invalidates all L1 and A27L2 cache lines.
    pub(crate) fn flush_all(&self) -> runtime::Result<()> {
        let time_source =
            crate::platform::time_source().ok_or(runtime::Error::NotEnoughResources)?;
        // Flush L1 before issuing the corresponding L2-wide operation.
        self.soc.write_back_and_invalidate_l1_all();
        self.l2_command(L2Command::WriteBackAndInvalidateAll, time_source)
    }

    pub(crate) fn write_back_range(
        &self,
        start: usize,
        length: usize,
        memory: &SupervisorMemory,
    ) -> runtime::Result<()> {
        self.maintain_range(start, length, RangeOperation::WriteBack, memory)
    }

    pub(crate) fn invalidate_range(
        &self,
        start: usize,
        length: usize,
        memory: &SupervisorMemory,
    ) -> runtime::Result<()> {
        self.maintain_range(start, length, RangeOperation::Invalidate, memory)
    }

    fn maintain_range(
        &self,
        start: usize,
        length: usize,
        operation: RangeOperation,
        memory: &SupervisorMemory,
    ) -> runtime::Result<()> {
        if length == 0 {
            return Ok(());
        }
        let end = start.checked_add(length).ok_or(runtime::Error::Overflow)?;
        let first = start & !(CACHE_LINE_SIZE - 1);
        let last = end
            .checked_add(CACHE_LINE_SIZE - 1)
            .ok_or(runtime::Error::Overflow)?
            & !(CACHE_LINE_SIZE - 1);
        memory.check_range(PhysAddrRange::new(
            PhysAddr::new(first),
            PhysAddr::new(last),
        )?)?;
        // M-mode cache addresses are physical even when satp is enabled.
        if operation == RangeOperation::WriteBack && length >= LARGE_FLUSH_THRESHOLD {
            return self.flush_all();
        }
        let time_source =
            crate::platform::time_source().ok_or(runtime::Error::NotEnoughResources)?;

        for address in (first..last).step_by(CACHE_LINE_SIZE) {
            let partial = address < start || end - address < CACHE_LINE_SIZE;
            let (l1_operation, l2_command) = if operation == RangeOperation::WriteBack {
                (L1Operation::WriteBack, L2Command::WriteBackRange)
            } else if partial {
                (
                    L1Operation::WriteBackAndInvalidate,
                    L2Command::WriteBackAndInvalidateRange,
                )
            } else {
                (L1Operation::Invalidate, L2Command::InvalidateRange)
            };
            // The full cache-line span was checked against supervisor RAM;
            // partial lines retain surrounding dirty bytes by writeback.
            self.soc.maintain_a27l2_line(address, l1_operation);
            let line_address = u32::try_from(address).map_err(|_| runtime::Error::InvalidArgs)?;
            self.write_u32(CacheRegister::LineAddress, line_address)?;
            self.l2_command(l2_command, time_source)?;
        }
        Ok(())
    }

    fn l2_command(&self, command: L2Command, time_source: &dyn TimeSource) -> runtime::Result<()> {
        self.registers.synchronize();
        self.write_u32(CacheRegister::Command, command as u32)?;
        let start = time_source.read_time_low() as u32;
        loop {
            match self.read_u32(CacheRegister::Status)? & STATUS_STATE_MASK {
                0 => {
                    self.registers.synchronize();
                    return Ok(());
                }
                STATUS_BUSY
                    if (time_source.read_time_low() as u32).wrapping_sub(start)
                        < COMMAND_TIMEOUT_TICKS => {}
                _ => return Err(runtime::Error::NotEnoughResources),
            }
            core::hint::spin_loop();
        }
    }

    fn read_u32(&self, register: CacheRegister) -> runtime::Result<u32> {
        self.registers
            .read::<u32>(register as usize)
            .map(u32::from_le)
    }

    fn write_u32(&self, register: CacheRegister, value: u32) -> runtime::Result<()> {
        self.registers.write(register as usize, value.to_le())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RangeOperation {
    WriteBack,
    Invalidate,
}
