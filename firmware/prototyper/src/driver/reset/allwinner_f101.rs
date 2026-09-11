//! Allwinner F101 watchdog reboot.
//!
//! Follows the CFG/MODE sequence and 1 ms delay in the [F101 OpenSBI platform].
//! The loader must leave the watchdog clock and architectural time counter
//! running. Cold and warm reboot use the same 0.5-second watchdog timeout.
//! Poweroff requires a separate peripheral.
//!
//! Runtime acquires the register window once. The SBI reset mutex serializes
//! this sequence; no other firmware or supervisor watchdog driver may program
//! these registers concurrently.
//!
//! [F101 OpenSBI platform]: https://github.com/YuzukiHD/opensbi/blob/08cbbe0bf6d2aaf0e8dbef8c548f4befdf3b7a7e/platform/generic/allwinner/sun252i-f101.c

use core::mem::{align_of, size_of};
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use super::{ResetBackend, ResetError, ResetRequest, ResetType};

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

pub(crate) struct F101Watchdog {
    registers: MmioRegion,
    delay_ticks: u64,
}

pub(in crate::driver) fn bind(
    registers: DeviceRegisterRange,
    timebase_frequency_hz: Option<u32>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<F101Watchdog> {
    let frequency = timebase_frequency_hz
        .filter(|frequency| *frequency != 0)
        .ok_or(runtime::Error::InvalidArgs)?;
    let registers = registers.subrange(0, REGISTER_SPAN)?;
    if !registers.start().is_aligned_to(align_of::<u32>()) {
        return Err(runtime::Error::InvalidArgs);
    }
    Ok(F101Watchdog {
        registers: memory.acquire_mmio(registers)?,
        // Round up so the delay is at least one millisecond.
        delay_ticks: u64::from(frequency).div_ceil(1_000),
    })
}

impl F101Watchdog {
    fn read(&self, register: Register) -> runtime::Result<u32> {
        self.registers
            .read::<u32>(register as usize)
            .map(u32::from_le)
    }

    fn write(&mut self, register: Register, value: u32) -> runtime::Result<()> {
        self.registers.write(register as usize, value.to_le())?;
        // Order each device write before the next register access or delay.
        riscv::asm::fence();
        Ok(())
    }

    fn start_reset(&mut self) -> runtime::Result<()> {
        riscv::asm::fence();
        // Clear CFG bit 0 before the delay, preserving its other fields.
        let config = self.read(Register::Config)?;
        self.write(Register::Config, (config & !CONFIG_ENABLE) | UPDATE_KEY)?;
        let start = riscv::register::time::read64();
        while riscv::register::time::read64().wrapping_sub(start) < self.delay_ticks {
            core::hint::spin_loop();
        }

        // Select system reset and the reference implementation's 0.5 s timeout.
        self.write(Register::Config, UPDATE_KEY | CONFIG_ENABLE)?;
        self.write(Register::Mode, UPDATE_KEY)?;
        let mode = self.read(Register::Mode)?;
        self.write(Register::Mode, mode | MODE_ENABLE | UPDATE_KEY)
    }
}

impl ResetBackend for F101Watchdog {
    type Request = ();

    fn prepare_reset(&self, req: ResetRequest) -> Option<Self::Request> {
        // The watchdog has no encoding for the reset reason.
        matches!(
            req.reset_type,
            ResetType::ColdReboot | ResetType::WarmReboot
        )
        .then_some(())
    }

    fn system_reset(&mut self, (): Self::Request) -> ResetError {
        if self.start_reset().is_err() {
            return ResetError::Failed;
        }
        // Reset is asynchronous. Park while the watchdog counts down.
        loop {
            riscv::asm::wfi();
        }
    }
}
