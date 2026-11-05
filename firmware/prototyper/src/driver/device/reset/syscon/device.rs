//! Syscon reset actions and MMIO command execution.

use alloc::sync::Arc;
use core::convert::Infallible;
use runtime::{
    Error, Result,
    memory::{MemoryRegistry, MmioRegion},
};

use super::super::{ResetDevice, ResetReason, ResetRequest, ResetType};
use super::description::{ActionDesc, Desc};

/// A syscon reset device with optional poweroff and reboot actions.
pub(super) struct SysconReset {
    poweroff: Option<SysconAction>,
    reboot: Option<SysconAction>,
}

impl SysconReset {
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let poweroff = description
            .poweroff
            .map(|description| SysconAction::init(description, memory, None))
            .transpose()?;
        let reboot = description
            .reboot
            .map(|description| SysconAction::init(description, memory, poweroff.as_ref()))
            .transpose()?;
        Ok(Self { poweroff, reboot })
    }
}

impl ResetDevice for SysconReset {
    fn system_reset(&mut self, request: ResetRequest) -> Result<Infallible> {
        match (request.reset_type(), request.reset_reason()) {
            (ResetType::Shutdown, ResetReason::NoReason | ResetReason::SystemFailure) => self
                .poweroff
                .as_ref()
                .map_or(Err(Error::InvalidArgs), SysconAction::execute),
            (
                ResetType::ColdReboot | ResetType::WarmReboot,
                ResetReason::NoReason | ResetReason::SystemFailure,
            ) => self
                .reboot
                .as_ref()
                .map_or(Err(Error::InvalidArgs), SysconAction::execute),
            _ => Err(Error::InvalidArgs),
        }
    }
}

/// One acquired syscon word and its masked update.
struct SysconAction {
    registers: Arc<MmioRegion>,
    value: u32,
    mask: u32,
}

impl SysconAction {
    fn init(
        description: ActionDesc,
        memory: &mut MemoryRegistry,
        shared: Option<&Self>,
    ) -> Result<Self> {
        let ActionDesc {
            registers: range,
            value,
            mask,
            ..
        } = description;
        // Poweroff and reboot can target the same word.
        // Reuse its mapping because the registry rejects overlapping acquisitions.
        let registers = match shared {
            Some(action)
                if action.registers.range().start() == range.start()
                    && action.registers.range().end() == range.end() =>
            {
                Arc::clone(&action.registers)
            }
            _ => Arc::new(memory.acquire(range)?),
        };
        Ok(Self {
            registers,
            value,
            mask,
        })
    }

    fn execute(&self) -> Result<Infallible> {
        // Order earlier memory and device accesses before issuing reset.
        runtime::instruction::fence();
        let Ok(old) = self.registers.read::<u32>(0) else {
            return Err(Error::IoError);
        };
        let value = (u32::from_le(old) & !self.mask) | (self.value & self.mask);
        if self.registers.write(0, value.to_le()).is_err() {
            return Err(Error::IoError);
        }
        runtime::instruction::fence();
        // A successful reset never returns; reaching this point is an I/O
        // failure.
        Err(Error::IoError)
    }
}
