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
use runtime::{
    Error, PlatformView, Result,
    memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion},
};

use alloc::boxed::Box;

use crate::devicetree::{EnabledNode, try_for_each_enabled_node};
use crate::driver::device::Driver;

use super::super::{Description, Reset, ResetDevice, ResetError, ResetRequest, ResetType};

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

/// Driver for the Allwinner V105 watchdog.
pub(in crate::driver::reset) struct V105;

pub(in crate::driver::reset) static WATCHDOG_V105: V105 = V105;

impl Driver<Reset> for V105 {
    fn compatibles(&self) -> &'static [&'static str] {
        &[COMPATIBLE]
    }

    fn parse<'a, 'tree>(
        &self,
        platform: &'a PlatformView<'tree>,
        device: EnabledNode<'a, 'tree>,
        _compatible: &str,
    ) -> Result<Option<Description>> {
        let node = device.node();
        let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
        let reset_gate = platform
            .sunxi_rtc_v203_gprcm(node)?
            .or(reset_gate(platform)?);
        Ok(Some(Description::new(
            "allwinner_watchdog_v105",
            registers,
            move |memory| {
                Ok(Box::new(V105Watchdog::bind(registers, reset_gate, memory)?)
                    as Box<dyn ResetDevice>)
            },
        )))
    }
}

fn reset_gate(platform: &PlatformView<'_>) -> Result<Option<DeviceRegisterRange>> {
    let mut gate = None;
    try_for_each_enabled_node(platform.root(), &mut |node, _| {
        if gate.is_none() {
            gate = platform.sunxi_rtc_v203_gprcm(node.node())?;
        }
        Ok(())
    })?;
    Ok(gate)
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
    ) -> Result<Self> {
        let registers = registers.subrange(0, REGISTER_SPAN)?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        let registers = memory.acquire(registers)?;
        let reset_gate = gprcm
            .map(|registers| {
                let gate = registers.subrange(RESET_GATE_OFFSET, size_of::<u32>())?;
                if !gate.start().is_aligned_to(align_of::<u32>()) {
                    return Err(Error::InvalidArgs);
                }
                memory.acquire(gate)
            })
            .transpose()?;
        Ok(Self {
            registers,
            reset_gate,
        })
    }

    fn start_reset(&mut self) -> Result<()> {
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

impl ResetDevice for V105Watchdog {
    fn priority(&self) -> u32 {
        208
    }

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
