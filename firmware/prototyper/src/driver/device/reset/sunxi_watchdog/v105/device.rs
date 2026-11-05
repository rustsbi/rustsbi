//! Sunxi V105 watchdog MMIO device.

use core::{
    convert::Infallible,
    mem::{align_of, size_of},
};
use runtime::{
    Error, Result,
    memory::{MemoryRegistry, MmioRegion},
};

use super::super::super::{ResetDevice, ResetReason, ResetRequest, ResetType};
use super::description::Desc;

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

/// A V105 watchdog bound to its MMIO window and optional RTC reset gate.
pub(super) struct V105Watchdog {
    registers: MmioRegion,
    reset_gate: Option<MmioRegion>,
}

impl V105Watchdog {
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let Desc {
            registers,
            reset_gate,
        } = description;
        let registers = registers.subrange(0, REGISTER_SPAN)?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        let registers = memory.acquire(registers)?;
        let reset_gate = reset_gate
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
        runtime::instruction::fence();
        if let Some(gate) = &self.reset_gate {
            let value = u32::from_le(gate.read::<u32>(0)?) | RESET_GATE_ENABLE;
            gate.write(0, value.to_le())?;
            runtime::instruction::fence();
        }

        // Stop the watchdog before issuing the software reset.
        // Preserve the other writable MODE fields and rewrite the update key.
        let mode = u32::from_le(self.registers.read::<u32>(Register::Mode as usize)?);
        let stopped = (mode & MODE_WRITABLE_BITS) | UPDATE_KEY;
        self.registers
            .write(Register::Mode as usize, stopped.to_le())?;
        runtime::instruction::fence();

        // V105 has a dedicated immediate reset command; no timer is required.
        self.registers.write(
            Register::SoftwareReset as usize,
            (UPDATE_KEY | SOFTWARE_RESET_TRIGGER).to_le(),
        )?;
        runtime::instruction::fence();
        Ok(())
    }
}

impl ResetDevice for V105Watchdog {
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
        loop {
            runtime::instruction::wait_for_interrupt();
        }
    }
}
