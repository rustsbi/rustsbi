//! Allwinner V104 watchdog reset driver.
//!
//! Binding claims the register window once. [`super::super::ResetController`]
//! serializes calls through this driver only; platform integration must ensure no
//! other firmware or supervisor watchdog driver programs the same registers.
//! The watchdog clock and Runtime's selected time source must remain running
//! during a reset request.
//!
//! # References
//!
//! - Reference implementation: [F101 OpenSBI platform](https://github.com/YuzukiHD/opensbi/blob/08cbbe0bf6d2aaf0e8dbef8c548f4befdf3b7a7e/platform/generic/allwinner/sun252i-f101.c)
//!   — CFG/MODE sequence, update key, and required one-millisecond delay.

use core::mem::{align_of, size_of};
use runtime::Result;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};
use runtime::timer::Timer;

use crate::devicetree::EnabledNode;

use super::super::registry::{self, ResetDriver};
use super::super::{ResetDevice, ResetError, ResetRequest, ResetType};

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
const COMPATIBLES: [&str; 2] = ["allwinner,sun20i-d1-wdt", "allwinner,wdt-v104"];

/// Devicetree description for the Sunxi V104 watchdog.
#[derive(Default)]
pub(in crate::driver::reset) struct V104Driver {
    registers: Option<DeviceRegisterRange>,
}

impl ResetDriver for V104Driver {
    fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        discovered: EnabledNode<'_, '_>,
    ) -> Result<()> {
        let Some(compatibles) = discovered.compatible() else {
            return Ok(());
        };
        if !compatibles
            .all()
            .any(|compatible| COMPATIBLES.contains(&compatible))
        {
            return Ok(());
        }
        let node = discovered.node();
        let registers = registry::primary_registers(platform, node)?;
        registry::set_once(&mut self.registers, registers)
    }

    fn has_device(&self) -> bool {
        self.registers.is_some()
    }

    fn bind(
        &self,
        memory: &mut MemoryRegistry,
        timebase_frequency_hz: Option<u32>,
    ) -> Result<alloc::boxed::Box<dyn ResetDevice>> {
        let registers = self.registers.ok_or(runtime::Error::InvalidArgs)?;
        Ok(alloc::boxed::Box::new(V104Watchdog::bind(
            registers,
            timebase_frequency_hz,
            memory,
        )?))
    }

    fn log_summary(&self) {
        if let Some(registers) = self.registers {
            info!(
                "{:<30}: Available (Sunxi WDT V104 @ 0x{:x})",
                "Platform Reset Extension",
                registers.start().as_usize(),
            );
        }
    }
}

struct V104Watchdog {
    registers: MmioRegion,
    delay_ticks: u64,
}

impl V104Watchdog {
    fn bind(
        registers: DeviceRegisterRange,
        timebase_frequency_hz: Option<u32>,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        let frequency = timebase_frequency_hz
            .filter(|frequency| *frequency != 0)
            .ok_or(runtime::Error::InvalidArgs)?;
        let registers = registers.subrange(0, REGISTER_SPAN)?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(runtime::Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire_mmio(registers)?,
            // Round up so the delay is at least one millisecond.
            delay_ticks: u64::from(frequency).div_ceil(DELAY_TICKS_1MS_DIVISOR),
        })
    }

    fn read(&self, register: Register) -> runtime::Result<u32> {
        self.registers
            .read::<u32>(register as usize)
            .map(u32::from_le)
    }

    fn write(&mut self, register: Register, value: u32) -> runtime::Result<()> {
        self.registers.write(register as usize, value.to_le())?;
        // Order each device write before the next register access or delay.
        self.registers.synchronize();
        Ok(())
    }

    fn start_reset(&mut self) -> core::result::Result<(), ResetError> {
        let time_error = |error| {
            error!("V104 watchdog: reset time source failed: {error:?}");
            ResetError::Failed
        };
        let register_error = |error| {
            error!("V104 watchdog: reset register access failed: {error}");
            ResetError::Failed
        };
        let timer = Timer::current().map_err(time_error)?;
        self.registers.synchronize();
        // Stop an active watchdog before synchronizing its reset output.
        self.write(Register::Mode, UPDATE_KEY)
            .map_err(register_error)?;
        // Clear CFG bit 0 before the delay, preserving its other fields.
        let config = self.read(Register::Config).map_err(register_error)?;
        self.write(Register::Config, (config & !CONFIG_ENABLE) | UPDATE_KEY)
            .map_err(register_error)?;
        let start = timer.read_time().map_err(time_error)?;
        while timer.read_time().map_err(time_error)?.wrapping_sub(start) < self.delay_ticks {
            core::hint::spin_loop();
        }

        // Select system reset and the shortest hardware timeout (encoding 0).
        self.write(Register::Config, UPDATE_KEY | CONFIG_ENABLE)
            .map_err(register_error)?;
        let mode = self.read(Register::Mode).map_err(register_error)?;
        self.write(Register::Mode, mode | MODE_ENABLE | UPDATE_KEY)
            .map_err(register_error)
    }
}

impl ResetDevice for V104Watchdog {
    fn system_reset(&mut self, request: ResetRequest) -> ResetError {
        // The watchdog has no encoding for the reset reason.
        if !matches!(
            request.reset_type(),
            ResetType::ColdReboot | ResetType::WarmReboot
        ) {
            return ResetError::InvalidRequest;
        }
        if self.start_reset().is_err() {
            return ResetError::Failed;
        }
        // Reset is asynchronous. Park while the watchdog counts down.
        loop {
            riscv::asm::wfi();
        }
    }
}
