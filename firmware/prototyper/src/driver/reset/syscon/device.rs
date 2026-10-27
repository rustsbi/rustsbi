//! Bound syscon reset actions and MMIO command execution.

use alloc::sync::Arc;
use core::mem::{align_of, size_of};
use runtime::{
    Error, Result,
    memory::{MemoryRegistry, MmioRegion},
};

use super::super::{ResetDevice, ResetError, ResetReason, ResetRequest, ResetType};
use super::description::ActionDescription;

/// One backend retains both actions under the SBI adapter's device lock.
pub(super) struct SysconReset {
    poweroff: Option<SysconAction>,
    reboot: Option<SysconAction>,
}

impl SysconReset {
    pub(super) fn bind(
        poweroff: Option<ActionDescription>,
        reboot: Option<ActionDescription>,
        memory: &mut MemoryRegistry,
    ) -> Result<Self> {
        let same_word = poweroff
            .zip(reboot)
            .is_some_and(|(poweroff, reboot)| poweroff.registers() == reboot.registers());
        let poweroff = poweroff
            .map(|description| SysconAction::bind(description, None, memory))
            .transpose()?;
        let shared = if same_word {
            poweroff
                .as_ref()
                .map(|action| Arc::clone(&action.registers))
        } else {
            None
        };
        let reboot = reboot
            .map(|description| SysconAction::bind(description, shared, memory))
            .transpose()?;
        Ok(Self { poweroff, reboot })
    }
}

impl ResetDevice for SysconReset {
    fn system_reset(&mut self, request: ResetRequest) -> ResetError {
        if !matches!(
            request.reset_reason(),
            ResetReason::NoReason | ResetReason::SystemFailure
        ) {
            return ResetError::InvalidRequest;
        }
        let action = match request.reset_type() {
            ResetType::Shutdown => &self.poweroff,
            ResetType::ColdReboot | ResetType::WarmReboot => &self.reboot,
            ResetType::VendorSpecific(_) => return ResetError::InvalidRequest,
        };
        action
            .as_ref()
            .map_or(ResetError::InvalidRequest, SysconAction::execute)
    }
}

/// One acquired syscon word and its masked update.
///
/// Actions that resolve to the same word share one acquisition.
struct SysconAction {
    registers: Arc<MmioRegion>,
    value: u32,
    mask: u32,
}

impl SysconAction {
    fn bind(
        description: ActionDescription,
        shared: Option<Arc<MmioRegion>>,
        memory: &mut MemoryRegistry,
    ) -> Result<Self> {
        let (registers, value, mask) = description.into_parts();
        let range = registers.subrange(0, size_of::<u32>())?;
        if !range.start().is_aligned_to(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            registers: match shared {
                Some(registers) => registers,
                None => Arc::new(memory.acquire(range)?),
            },
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
