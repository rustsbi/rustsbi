//! Sunxi V104 watchdog MMIO device.

use core::{
    convert::Infallible,
    mem::{align_of, size_of},
};
use runtime::{
    Error, Result,
    memory::{MemoryRegistry, MmioRegion},
    timer,
};

use super::super::super::{ResetDevice, ResetReason, ResetRequest, ResetType};
use super::description::Desc;

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    Config = 0x14,
    Mode = 0x18,
}

const REGISTER_SPAN: usize = Register::Mode as usize + size_of::<u32>();
const UPDATE_KEY: u32 = 0x16aa << 16;
const CONFIG_ENABLE: u32 = 1 << 0;
const MODE_ENABLE: u32 = 1 << 0;
const DELAY_TICKS_1MS_DIVISOR: u64 = 1_000;

/// A V104 watchdog bound to its MMIO window and one-millisecond delay count.
pub(super) struct V104Watchdog {
    registers: MmioRegion,
    delay_ticks: u64,
}

impl V104Watchdog {
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let Desc {
            registers,
            timebase_frequency_hz,
        } = description;
        let frequency = timebase_frequency_hz
            .filter(|frequency| *frequency != 0)
            .ok_or(Error::InvalidArgs)?;
        let registers = registers.subrange(0, REGISTER_SPAN)?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire(registers)?,
            // Round up so the delay is at least one millisecond.
            delay_ticks: u64::from(frequency).div_ceil(DELAY_TICKS_1MS_DIVISOR),
        })
    }

    fn read(&self, register: Register) -> Result<u32> {
        self.registers
            .read::<u32>(register as usize)
            .map(u32::from_le)
    }

    fn write(&mut self, register: Register, value: u32) -> Result<()> {
        self.registers.write(register as usize, value.to_le())?;
        // Order each device write before the next register access or delay.
        runtime::instruction::fence();
        Ok(())
    }

    fn start_reset(&mut self) -> Result<()> {
        runtime::instruction::fence();
        // Stop an active watchdog before programming its reset mode.
        self.write(Register::Mode, UPDATE_KEY)?;
        // Clear the reset selection before the required delay,
        // preserving the other `CFG` fields.
        let config = self.read(Register::Config)?;
        self.write(Register::Config, (config & !CONFIG_ENABLE) | UPDATE_KEY)?;
        let start = timer::now()?.ok_or(Error::NotSupported)?;
        while timer::now()?.is_some_and(|now| now.wrapping_sub(start) < self.delay_ticks) {
            core::hint::spin_loop();
        }

        // Select system reset and the shortest hardware timeout (encoding 0).
        self.write(Register::Config, UPDATE_KEY | CONFIG_ENABLE)?;
        let mode = self.read(Register::Mode)?;
        self.write(Register::Mode, mode | MODE_ENABLE | UPDATE_KEY)
    }
}

impl ResetDevice for V104Watchdog {
    fn system_reset(&mut self, request: ResetRequest) -> Result<Infallible> {
        // The watchdog can trigger a reset but cannot power off the system,
        // and it has no field for an SBI- or vendor-specific reason.
        if !matches!(
            (request.reset_type(), request.reset_reason()),
            (
                ResetType::ColdReboot | ResetType::WarmReboot,
                ResetReason::NoReason | ResetReason::SystemFailure,
            )
        ) {
            return Err(Error::InvalidArgs);
        }
        self.start_reset().map_err(|_| Error::IoError)?;
        // Reset is asynchronous. Park while the watchdog counts down.
        loop {
            runtime::instruction::wait_for_interrupt();
        }
    }
}
