//! Allwinner V104 watchdog reset driver.
//!
//! Runtime acquires the register window once. The SBI reset mutex serializes
//! calls through this driver only; platform integration must ensure that no
//! other firmware or supervisor watchdog driver programs the same registers.
//! The loader must leave the watchdog clock and architectural time counter
//! running.
//!
//! # References
//!
//! - Reference implementation: [F101 OpenSBI platform](https://github.com/YuzukiHD/opensbi/blob/08cbbe0bf6d2aaf0e8dbef8c548f4befdf3b7a7e/platform/generic/allwinner/sun252i-f101.c)
//!   — CFG/MODE sequence, update key, and required one-millisecond delay.

use core::mem::{align_of, size_of};
use runtime::Result;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::devicetree::EnabledNode;

use super::super::registry::{self, BindResources, ResetDriver};
use super::super::{ResetBackend, ResetError, ResetRequest, ResetType};

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
        resources: &mut BindResources<'_>,
    ) -> Result<alloc::boxed::Box<dyn ResetBackend>> {
        let registers = self.registers.ok_or(runtime::Error::InvalidArgs)?;
        let timebase_frequency_hz = resources.timebase_frequency_hz();
        Ok(alloc::boxed::Box::new(V104Watchdog::bind(
            registers,
            timebase_frequency_hz,
            resources.memory(),
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
        riscv::asm::fence();
        Ok(())
    }

    fn start_reset(&mut self) -> runtime::Result<()> {
        riscv::asm::fence();
        // Stop an active watchdog before synchronizing its reset output.
        self.write(Register::Mode, UPDATE_KEY)?;
        // Clear CFG bit 0 before the delay, preserving its other fields.
        let config = self.read(Register::Config)?;
        self.write(Register::Config, (config & !CONFIG_ENABLE) | UPDATE_KEY)?;
        let start = riscv::register::time::read64();
        while riscv::register::time::read64().wrapping_sub(start) < self.delay_ticks {
            core::hint::spin_loop();
        }

        // Select system reset and the shortest hardware timeout (encoding 0).
        self.write(Register::Config, UPDATE_KEY | CONFIG_ENABLE)?;
        let mode = self.read(Register::Mode)?;
        self.write(Register::Mode, mode | MODE_ENABLE | UPDATE_KEY)
    }
}

impl ResetBackend for V104Watchdog {
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
