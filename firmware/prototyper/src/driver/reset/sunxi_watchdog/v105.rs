//! Allwinner V105 watchdog reset driver.
//!
//! The driver stops the watchdog before issuing its dedicated software-reset
//! command. When the Platform Description supplies an RTC V203 `gprcm_reg`
//! window, it also enables the associated reset gate. Discovery and binding do
//! not modify hardware; [`super::super::ResetDevice`] serializes the command
//! sequence across harts.
//!
//! # References
//!
//! - Vendor implementation: [Tina V821 watchdog driver](https://github.com/sam-yangjj/tina-v821-v1.3-bsp/blob/4f82c3bed72342d306e1b14755f9618a6ca7a504/drivers/watchdog/sunxi_wdt.c)
//!   — V105 register offsets, update key, and stop-then-trigger ordering.

use core::mem::{align_of, size_of};
use runtime::Result;
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use crate::devicetree::EnabledNode;

use super::super::registry::{self, BindResources, ResetDriver};
use super::super::{ResetBackend, ResetError, ResetRequest, ResetType};

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    SoftwareReset = 0x08,
    Mode = 0x18,
}

const REGISTER_SPAN: usize = Register::Mode as usize + size_of::<u32>();
const UPDATE_KEY: u32 = 0x16aa << 16;
const RESET_GATE_ENABLE: u32 = 1 << 3;
const RESET_GATE_OFFSET: usize = 0x1c;
const MODE_WRITABLE_BITS: u32 = 0x0000_fffe;
const SOFTWARE_RESET_TRIGGER: u32 = 1;
const COMPATIBLE: &str = "allwinner,wdt-v105";

/// Devicetree description for the Sunxi V105 watchdog and its optional RTC gate.
#[derive(Default)]
pub(in crate::driver::reset) struct V105Driver {
    registers: Option<DeviceRegisterRange>,
    reset_gate: Option<DeviceRegisterRange>,
}

impl ResetDriver for V105Driver {
    fn probe(
        &mut self,
        platform: &runtime::PlatformView<'_>,
        discovered: EnabledNode<'_, '_>,
    ) -> Result<()> {
        let node = discovered.node();
        if let Some(registers) = platform.sunxi_rtc_v203_gprcm(node)? {
            registry::set_once(&mut self.reset_gate, registers)?;
        }

        let Some(compatibles) = discovered.compatible() else {
            return Ok(());
        };
        if !compatibles.all().any(|compatible| compatible == COMPATIBLE) {
            return Ok(());
        }
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
        Ok(alloc::boxed::Box::new(V105Watchdog::bind(
            registers,
            self.reset_gate,
            resources.memory(),
        )?))
    }

    fn log_summary(&self) {
        if let Some(registers) = self.registers {
            info!(
                "{:<30}: Available (Sunxi WDT V105 @ 0x{:x})",
                "Platform Reset Extension",
                registers.start().as_usize(),
            );
        }
    }
}

struct V105Watchdog {
    registers: MmioRegion,
    reset_gate: Option<MmioRegion>,
}

impl V105Watchdog {
    fn bind(
        registers: DeviceRegisterRange,
        gprcm: Option<DeviceRegisterRange>,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        let registers = registers.subrange(0, REGISTER_SPAN)?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(runtime::Error::InvalidArgs);
        }
        let registers = memory.acquire_mmio(registers)?;
        let reset_gate = gprcm
            .map(|registers| {
                let gate = registers.subrange(RESET_GATE_OFFSET, size_of::<u32>())?;
                if !gate.start().is_aligned_to(align_of::<u32>()) {
                    return Err(runtime::Error::InvalidArgs);
                }
                memory.acquire_mmio(gate)
            })
            .transpose()?;
        Ok(Self {
            registers,
            reset_gate,
        })
    }

    fn start_reset(&mut self) -> runtime::Result<()> {
        riscv::asm::fence();
        if let Some(gate) = &self.reset_gate {
            let value = u32::from_le(gate.read::<u32>(0)?) | RESET_GATE_ENABLE;
            gate.write(0, value.to_le())?;
            riscv::asm::fence();
        }

        // Retain MODE's writable low fields, clear enable, and replace the key.
        let mode = u32::from_le(self.registers.read::<u32>(Register::Mode as usize)?);
        let stopped = (mode & MODE_WRITABLE_BITS) | UPDATE_KEY;
        self.registers
            .write(Register::Mode as usize, stopped.to_le())?;
        riscv::asm::fence();

        // V105 has a dedicated immediate reset command; no timer is required.
        self.registers.write(
            Register::SoftwareReset as usize,
            (UPDATE_KEY | SOFTWARE_RESET_TRIGGER).to_le(),
        )?;
        riscv::asm::fence();
        Ok(())
    }
}

impl ResetBackend for V105Watchdog {
    fn system_reset(&mut self, request: ResetRequest) -> ResetError {
        if !matches!(
            request.reset_type(),
            ResetType::ColdReboot | ResetType::WarmReboot
        ) {
            return ResetError::InvalidRequest;
        }
        if self.start_reset().is_err() {
            return ResetError::Failed;
        }
        loop {
            riscv::asm::wfi();
        }
    }
}
