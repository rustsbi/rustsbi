//! V821 A27L2 cache driver.
//!
//! # References
//!
//! - Vendor implementation: [V821 SPL cache support](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/spl/arch/riscv/cpu/ads_rv32/mmu.c)
//!   — A27L2 MMIO layout, command encodings, cache-line size, and initialization
//!   bits.

#![forbid(unsafe_code)]

use runtime::{
    Error, Result,
    memory::{MemoryRegistry, MmioRegion, PhysAddr, PhysAddrRange, SupervisorMemory},
    soc::allwinner::v821::{A27L2LineOperation as L1Operation, AndesStatusRegister, V821Csr},
    timer,
};

use super::description::Desc;

/// The A27L2 cache-maintenance MMIO device.
///
/// V821 preparation requires exactly one enabled hart,
/// so its CSR/MMIO command sequence needs no inter-hart mutex.
pub(crate) struct A27L2Cache {
    csr: V821Csr,
    registers: CacheRegisters,
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

impl CacheRegister {
    const fn offset(self) -> usize {
        self as usize
    }
}

/// BSP-required A27L2 control bits set during firmware initialization.
const L2_CONTROL_INIT_MASK: u32 = (1 << 13) | (1 << 10) | 1;
const CACHE_LINE_SIZE: usize = 64;
const LARGE_FLUSH_THRESHOLD: usize = 128 * 1024;
const COMMAND_TIMEOUT_TICKS: u32 = 4_000_000;
const STATUS_STATE_MASK: u32 = 0xf;
const STATUS_BUSY: u32 = 1;

impl A27L2Cache {
    pub(crate) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let registers = CacheRegisters {
            mmio: memory.acquire(description.registers)?,
        };
        let control = registers.read_u32(CacheRegister::Control)?;
        registers.write_u32(CacheRegister::Control, control | L2_CONTROL_INIT_MASK)?;
        runtime::instruction::fence();
        Ok(Self {
            csr: description.soc.csr(),
            registers,
        })
    }

    /// Reads an Andes machine-control status register.
    pub(crate) fn read_status(&self, register: AndesStatusRegister) -> usize {
        self.csr.read_status(register)
    }

    /// Flushes the complete A27L2 cache.
    pub(crate) fn flush_all(&self) -> Result<()> {
        // Flush L1 before issuing the corresponding L2-wide operation.
        self.csr.write_back_and_invalidate_l1_all();
        self.l2_command(L2Command::WriteBackAndInvalidateAll)
    }

    pub(crate) fn write_back_range(
        &self,
        start: usize,
        length: usize,
        memory: &SupervisorMemory,
    ) -> Result<()> {
        self.maintain_range(start, length, RangeOperation::WriteBack, memory)
    }

    pub(crate) fn invalidate_range(
        &self,
        start: usize,
        length: usize,
        memory: &SupervisorMemory,
    ) -> Result<()> {
        self.maintain_range(start, length, RangeOperation::Invalidate, memory)
    }

    /// Performs one validated A27L2 cache-range operation.
    fn maintain_range(
        &self,
        start: usize,
        length: usize,
        operation: RangeOperation,
        memory: &SupervisorMemory,
    ) -> Result<()> {
        if length == 0 {
            return Ok(());
        }
        let end = start.checked_add(length).ok_or(Error::Overflow)?;
        let first = start & !(CACHE_LINE_SIZE - 1);
        let last = end
            .checked_add(CACHE_LINE_SIZE - 1)
            .ok_or(Error::Overflow)?
            & !(CACHE_LINE_SIZE - 1);
        memory.check_range(PhysAddrRange::new(
            PhysAddr::new(first),
            PhysAddr::new(last),
        )?)?;
        // M-mode cache addresses are physical even when satp is enabled.
        if operation == RangeOperation::WriteBack && length >= LARGE_FLUSH_THRESHOLD {
            return self.flush_all();
        }

        for address in (first..last).step_by(CACHE_LINE_SIZE) {
            let partial = address < start || end - address < CACHE_LINE_SIZE;
            let (l1_operation, l2_command) = match (operation, partial) {
                (RangeOperation::WriteBack, _) => {
                    (L1Operation::WriteBack, L2Command::WriteBackRange)
                }
                (RangeOperation::Invalidate, true) => (
                    L1Operation::WriteBackAndInvalidate,
                    L2Command::WriteBackAndInvalidateRange,
                ),
                (RangeOperation::Invalidate, false) => {
                    (L1Operation::Invalidate, L2Command::InvalidateRange)
                }
            };
            // The full cache-line span was checked against supervisor RAM;
            // partial lines retain surrounding dirty bytes by writeback.
            self.csr.maintain_a27l2_line(address, l1_operation);
            let line_address = u32::try_from(address).map_err(|_| Error::InvalidArgs)?;
            self.registers
                .write_u32(CacheRegister::LineAddress, line_address)?;
            self.l2_command(l2_command)?;
        }
        Ok(())
    }

    fn l2_command(&self, command: L2Command) -> Result<()> {
        runtime::instruction::fence();
        self.registers
            .write_u32(CacheRegister::Command, command as u32)?;
        let read_time_fn = || {
            timer::now()
                .map(|value| value as u32)
                .ok_or(Error::NotEnoughResources)
        };
        let start = read_time_fn()?;
        loop {
            match self.registers.read_u32(CacheRegister::Status)? & STATUS_STATE_MASK {
                0 => {
                    runtime::instruction::fence();
                    return Ok(());
                }
                STATUS_BUSY if read_time_fn()?.wrapping_sub(start) < COMMAND_TIMEOUT_TICKS => {}
                _ => return Err(Error::NotEnoughResources),
            }
            core::hint::spin_loop();
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum RangeOperation {
    WriteBack,
    Invalidate,
}

struct CacheRegisters {
    mmio: MmioRegion,
}

impl CacheRegisters {
    fn read_u32(&self, register: CacheRegister) -> Result<u32> {
        self.mmio.read::<u32>(register.offset()).map(u32::from_le)
    }

    fn write_u32(&self, register: CacheRegister, value: u32) -> Result<()> {
        self.mmio.write(register.offset(), value.to_le())
    }
}
