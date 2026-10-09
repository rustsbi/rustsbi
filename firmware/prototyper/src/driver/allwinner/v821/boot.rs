//! V821 boot0 SRAM handshake and PLMT clock control.
//!
//! Platform code decides when to perform these device operations.

use runtime::memory::MemoryRegistry;
use runtime::soc::allwinner::v821::AllwinnerV821Soc;

/// Lets the RTOS ISP driver reclaim boot0's SRAM after firmware moves to DRAM.
pub(crate) fn release_boot0_isp_sram(
    soc: AllwinnerV821Soc,
    memory: &mut MemoryRegistry,
) -> runtime::Result<()> {
    const BOOT0_ISP_SRAM_RELEASED: u32 = 1 << 0;

    let registers = memory.acquire_mmio(soc.boot0_isp_sram_release()?)?;
    registers.synchronize();
    let flags = registers.read::<u32>(0)?;
    registers.write(0, flags | BOOT0_ISP_SRAM_RELEASED)?;
    registers.synchronize();
    Ok(())
}

/// Enables the V821 CCU gate required before binding the PLMT timer.
pub(crate) fn enable_plmt_clock(
    soc: AllwinnerV821Soc,
    memory: &mut MemoryRegistry,
) -> runtime::Result<()> {
    const CLOCK_ENABLE: u32 = 1 << 31;

    let registers = memory.acquire_mmio(soc.plmt_clock()?)?;
    let value = u32::from_le(registers.read::<u32>(0)?);
    registers.write(0, (value | CLOCK_ENABLE).to_le())?;
    registers.synchronize();
    Ok(())
}
