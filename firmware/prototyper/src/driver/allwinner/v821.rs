//! V821 A27L2 cache and USB firmware drivers.
//!
//! Both devices are selected by the same V821 root capability. They stay
//! outside the platform-wide device set because only the V821 custom SBI
//! extensions consume them.
//!
//! # References
//!
//! - Vendor implementation: [V821 SPL cache support](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/spl/arch/riscv/cpu/ads_rv32/mmu.c)
//!   — A27L2 MMIO layout, command encodings, cache-line size, and initialization
//!   bits.
//! - Vendor register definitions: [V821 USB controller](https://github.com/sam-yangjj/tina-v821-v1.3-bsp/blob/4f82c3bed72342d306e1b14755f9618a6ca7a504/drivers/usb/sunxi_usb/include/sunxi_usb_bsp.h)
//!   — DMA word-address bypass register offset.

#![forbid(unsafe_code)]

use runtime::memory::{
    DeviceRegisterRange, MemoryRegistry, MmioRegion, PhysAddr, PhysAddrRange, SupervisorMemory,
};
use runtime::soc::allwinner::v821::{
    A27L2LineOperation as L1Operation, AllwinnerV821Soc, AndesStatusRegister, V821Csr,
};

/// The A27L2 cache-maintenance MMIO device.
///
/// V821 preparation requires exactly one enabled hart, so its CSR/MMIO
/// command sequence needs no inter-hart mutex.
pub(crate) struct A27L2Cache {
    csr: V821Csr,
    registers: CacheRegisters,
}

/// The USB controller's DMA word-address bypass register.
pub(crate) struct UsbDmaBypass {
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

impl CacheRegister {
    const fn offset(self) -> usize {
        self as usize
    }
}

/// BSP-required A27L2 control bits set during firmware binding.
const L2_CONTROL_INIT_MASK: u32 = (1 << 13) | (1 << 10) | 1;
const CACHE_LINE_SIZE: usize = 64;
const LARGE_FLUSH_THRESHOLD: usize = 128 * 1024;
const COMMAND_TIMEOUT_TICKS: u32 = 4_000_000;
const STATUS_STATE_MASK: u32 = 0xf;
const STATUS_BUSY: u32 = 1;

/// Enables the V821 CCU gate required by the PLMT timer.
pub(crate) fn enable_plmt_clock(
    soc: AllwinnerV821Soc,
    memory: &mut MemoryRegistry,
) -> runtime::Result<()> {
    const CLOCK_ENABLE: u32 = 1 << 31;

    let registers = memory.acquire_mmio(soc.plmt_clock()?)?;
    let value = u32::from_le(registers.read::<u32>(0)?);
    registers.write(0, (value | CLOCK_ENABLE).to_le())?;
    riscv::asm::fence();
    Ok(())
}

impl A27L2Cache {
    pub(crate) fn bind(
        soc: AllwinnerV821Soc,
        registers: DeviceRegisterRange,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        let registers = CacheRegisters {
            mmio: memory.acquire_mmio(registers)?,
        };
        let control = registers.read_u32(CacheRegister::Control)?;
        registers.write_u32(CacheRegister::Control, control | L2_CONTROL_INIT_MASK)?;
        riscv::asm::fence();
        Ok(Self {
            csr: soc.csr(),
            registers,
        })
    }

    /// Reads an Andes machine-control status register.
    pub(crate) fn read_status(&self, register: AndesStatusRegister) -> usize {
        self.csr.read_status(register)
    }

    /// Flushes the complete A27L2 cache.
    pub(crate) fn flush_all(&self) -> runtime::Result<()> {
        // Flush L1 before issuing the corresponding L2-wide operation.
        self.csr.write_back_and_invalidate_l1_all();
        self.l2_command(L2Command::WriteBackAndInvalidateAll)
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

    /// Performs one validated A27L2 cache-range operation.
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
            self.csr.maintain_a27l2_line(address, l1_operation);
            let line_address = u32::try_from(address).map_err(|_| runtime::Error::InvalidArgs)?;
            self.registers
                .write_u32(CacheRegister::LineAddress, line_address)?;
            self.l2_command(l2_command)?;
        }
        Ok(())
    }

    fn l2_command(&self, command: L2Command) -> runtime::Result<()> {
        riscv::asm::fence();
        self.registers
            .write_u32(CacheRegister::Command, command as u32)?;
        let timer = crate::driver::timer::get().ok_or(runtime::Error::NotEnoughResources)?;
        let read_time_fn = || {
            timer
                .read_time_low()
                .map(|value| value as u32)
                .ok_or(runtime::Error::NotEnoughResources)
        };
        let start = read_time_fn()?;
        loop {
            match self.registers.read_u32(CacheRegister::Status)? & STATUS_STATE_MASK {
                0 => {
                    riscv::asm::fence();
                    return Ok(());
                }
                STATUS_BUSY if read_time_fn()?.wrapping_sub(start) < COMMAND_TIMEOUT_TICKS => {}
                _ => return Err(runtime::Error::NotEnoughResources),
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

impl UsbDmaBypass {
    pub(crate) fn bind(
        registers: DeviceRegisterRange,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        Ok(Self {
            registers: memory.acquire_mmio(registers)?,
        })
    }

    /// Enables DMA word-address bypass after the supervisor enables USB clocks.
    ///
    /// Concurrent calls write the same idempotent value, so this operation
    /// needs no mutex; the fences remain ordered around each individual write.
    pub(crate) fn enable(&self) -> runtime::Result<()> {
        riscv::asm::fence();
        self.registers.write(0, 1u32)?;
        riscv::asm::fence();
        Ok(())
    }
}

struct CacheRegisters {
    mmio: MmioRegion,
}

impl CacheRegisters {
    fn read_u32(&self, register: CacheRegister) -> runtime::Result<u32> {
        self.mmio.read::<u32>(register.offset()).map(u32::from_le)
    }

    fn write_u32(&self, register: CacheRegister, value: u32) -> runtime::Result<()> {
        self.mmio.write(register.offset(), value.to_le())
    }
}
