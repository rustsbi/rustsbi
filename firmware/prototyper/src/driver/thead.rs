//! T-Head interrupt-controller register protocol.

use runtime::memory::{DeviceRegisterRange, MemoryRegistry};

/// Enables supervisor access to the T-Head PLIC during boot.
pub(crate) fn delegate_to_supervisor(
    registers: DeviceRegisterRange,
    memory: &mut MemoryRegistry,
) -> runtime::Result<()> {
    const CONTROL_OFFSET: usize = 0x1ffffc;
    let registers = memory.acquire_mmio(registers.subrange(CONTROL_OFFSET, size_of::<u32>())?)?;
    // PLIC_CTRL bit 0 permits S-mode accesses to the controller.
    registers.write(0, 1u32)
}
