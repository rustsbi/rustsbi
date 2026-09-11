//! Syscon poweroff and reboot through one masked 32-bit register update.
//!
//! Discovery resolves the register word, value and mask before binding. The
//! register must support ordinary reads and writes: read-clear, W1C and
//! write-only registers cannot use this backend. Each peripheral owns an independent
//! Runtime MMIO window; register windows must not overlap.
//!
//! Binding references:
//! - <https://www.kernel.org/doc/Documentation/devicetree/bindings/power/reset/syscon-poweroff.yaml>
//! - <https://www.kernel.org/doc/Documentation/devicetree/bindings/power/reset/syscon-reboot.yaml>

use core::mem::{align_of, size_of};
use runtime::memory::{DeviceRegisterRange, MemoryRegistry, MmioRegion};

use super::{ResetError, SysconPoweroff, SysconReboot};

/// A resolved syscon action, without access to its registers yet.
#[derive(Clone, Copy, Debug)]
pub(crate) struct SysconConfig {
    pub(crate) registers: DeviceRegisterRange,
    pub(crate) value: u32,
    pub(crate) mask: u32,
    pub(crate) priority: u32,
}

/// One acquired word and the masked update for a single peripheral.
pub(super) struct SysconWord {
    registers: MmioRegion,
    value: u32,
    mask: u32,
}

pub(in crate::driver) fn bind(
    poweroff: Option<SysconConfig>,
    reboot: Option<SysconConfig>,
    memory: &mut MemoryRegistry,
) -> runtime::Result<(Option<SysconPoweroff>, Option<SysconReboot>)> {
    let poweroff = poweroff
        .map(|config| SysconWord::bind(config, memory))
        .transpose()?;
    let reboot = reboot
        .map(|config| SysconWord::bind(config, memory))
        .transpose()?;
    Ok((
        poweroff.map(SysconPoweroff::new),
        reboot.map(SysconReboot::new),
    ))
}

impl SysconWord {
    /// Binds one action to its own register window.
    fn bind(config: SysconConfig, memory: &mut MemoryRegistry) -> runtime::Result<Self> {
        let range = config.registers.subrange(0, size_of::<u32>())?;
        if !range.start().is_aligned_to(align_of::<u32>()) {
            return Err(runtime::Error::InvalidArgs);
        }
        Ok(Self {
            registers: memory.acquire_mmio(range)?,
            value: config.value,
            mask: config.mask,
        })
    }

    pub(super) fn write_masked(&self) -> ResetError {
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
