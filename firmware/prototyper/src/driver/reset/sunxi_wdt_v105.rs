//! Allwinner BSP `allwinner,wdt-v105` software reset.
//!
//! Register offsets and stop-then-trigger ordering follow Tina V821's
//! `bsp/drivers/watchdog/sunxi_wdt.c` (driver version 1.0.5). CFG is at 0x14,
//! MODE at 0x18, and SOFT_RST at 0x08. SOFT_RST is issued as a command; it is
//! not read or updated through a generic read-modify-write operation.
//!
//! On V821, the original Avaota F1 OpenSBI additionally sets RTC
//! VDD_OFF_GATING bit 3. The RTC V203 `gprcm_reg` property supplies this
//! window; no SoC model check or fixed physical address is used. Discovery
//! accepts a single RTC V203 GPRCM window; ambiguous multiple windows fail.
//! Without this optional window, only the watchdog reset command is issued.
//!
//! The loader must leave the watchdog clock/reset accessible. The SBI reset
//! mutex serializes the sequence; other harts and the E907 must not program
//! the watchdog or reset gate concurrently. Neither binding nor probing
//! modifies hardware. Cold and warm reboot both reset the system.

use core::mem::{align_of, size_of};
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use super::{ResetBackend, ResetError, ResetRequest, ResetType};

#[repr(usize)]
#[derive(Clone, Copy)]
enum Register {
    SoftwareReset = 0x08,
    Mode = 0x18,
}

const REGISTER_SPAN: usize = Register::Mode as usize + size_of::<u32>();
const UPDATE_KEY: u32 = 0x16aa << 16;
const RESET_GATE_ENABLE: u32 = 1 << 3;

pub(crate) struct SunxiWdtV105 {
    registers: MmioRegion,
    reset_gate: Option<MmioRegion>,
}

pub(in crate::driver) fn bind(
    registers: DeviceRegisterRange,
    gprcm: Option<DeviceRegisterRange>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<SunxiWdtV105> {
    let registers = registers.subrange(0, REGISTER_SPAN)?;
    if !registers.start().is_aligned_to(align_of::<u32>()) {
        return Err(runtime::Error::InvalidArgs);
    }
    let registers = memory.acquire_mmio(registers)?;
    let reset_gate = gprcm
        .map(|registers| {
            let gate = registers.subrange(0x1c, size_of::<u32>())?;
            if !gate.start().is_aligned_to(align_of::<u32>()) {
                return Err(runtime::Error::InvalidArgs);
            }
            memory.acquire_mmio(gate)
        })
        .transpose()?;
    Ok(SunxiWdtV105 {
        registers,
        reset_gate,
    })
}

impl SunxiWdtV105 {
    fn start_reset(&mut self) -> runtime::Result<()> {
        riscv::asm::fence();
        if let Some(gate) = &self.reset_gate {
            let value = u32::from_le(gate.read::<u32>(0)?) | RESET_GATE_ENABLE;
            gate.write(0, value.to_le())?;
            riscv::asm::fence();
        }

        // Retain MODE's writable low fields, clear enable, and replace the key.
        let mode = u32::from_le(self.registers.read::<u32>(Register::Mode as usize)?);
        let stopped = (mode & 0x0000_fffe) | UPDATE_KEY;
        self.registers
            .write(Register::Mode as usize, stopped.to_le())?;
        riscv::asm::fence();

        // V105 has a dedicated immediate reset command; no timer is required.
        self.registers
            .write(Register::SoftwareReset as usize, (UPDATE_KEY | 1).to_le())?;
        riscv::asm::fence();
        Ok(())
    }
}

impl ResetBackend for SunxiWdtV105 {
    type Request = ();

    fn prepare_reset(&self, req: ResetRequest) -> Option<Self::Request> {
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
        loop {
            riscv::asm::wfi();
        }
    }
}
