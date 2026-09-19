//! Bound syscon reset actions and MMIO command execution.

use core::mem::{align_of, size_of};

use runtime::memory::{MemoryRegistry, MmioRegion};

use super::super::{ResetBackend, ResetError, ResetRequest, ResetType};
use super::description::ActionDescription;

/// One syscon family binding with optional poweroff and reboot actions.
pub(super) struct SysconReset {
    poweroff: Option<SysconAction>,
    reboot: Option<SysconAction>,
}

impl SysconReset {
    pub(super) fn bind(
        poweroff: Option<ActionDescription>,
        reboot: Option<ActionDescription>,
        memory: &mut MemoryRegistry,
    ) -> runtime::Result<Self> {
        let poweroff = poweroff
            .map(|description| SysconAction::bind(description, memory))
            .transpose()?;
        let reboot = reboot
            .map(|description| SysconAction::bind(description, memory))
            .transpose()?;
        Ok(Self { poweroff, reboot })
    }
}

impl ResetBackend for SysconReset {
    fn system_reset(&mut self, request: ResetRequest) -> ResetError {
        match request.reset_type() {
            ResetType::Shutdown => self
                .poweroff
                .as_ref()
                .map_or(ResetError::InvalidRequest, SysconAction::execute),
            ResetType::ColdReboot | ResetType::WarmReboot => self
                .reboot
                .as_ref()
                .map_or(ResetError::InvalidRequest, SysconAction::execute),
            ResetType::VendorSpecific(_) => ResetError::InvalidRequest,
        }
    }
}

/// One acquired syscon word and its masked update.
struct SysconAction {
    registers: MmioRegion,
    value: u32,
    mask: u32,
}

impl SysconAction {
    fn bind(description: ActionDescription, memory: &mut MemoryRegistry) -> runtime::Result<Self> {
        let (registers, value, mask) = description.into_parts();
        let range = registers.subrange(0, size_of::<u32>())?;
        if !range.start().is_aligned_to(align_of::<u32>()) {
            return Err(runtime::Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire_mmio(range)?,
            value,
            mask,
        })
    }

    fn execute(&self) -> ResetError {
        // Order earlier memory and device accesses before issuing reset.
        riscv::asm::fence();
        let Ok(old) = self.registers.read::<u32>(0) else {
            return ResetError::Failed;
        };
        let value = (u32::from_le(old) & !self.mask) | (self.value & self.mask);
        if self.registers.write(0, value.to_le()).is_err() {
            return ResetError::Failed;
        }
        riscv::asm::fence();
        // Successful reset never returns. Returning after issue is a failure.
        ResetError::Failed
    }
}
