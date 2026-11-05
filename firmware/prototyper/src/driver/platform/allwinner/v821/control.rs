//! V821 boot handshake and platform-specific devices.

#![forbid(unsafe_code)]

use runtime::{Result, memory::MemoryRegistry, soc::allwinner::v821::AllwinnerV821Soc};

/// Lets the RTOS ISP driver reclaim boot0's SRAM after firmware moves to DRAM.
pub(crate) fn release_boot0_isp_sram(
    soc: AllwinnerV821Soc,
    memory: &mut MemoryRegistry,
) -> Result<()> {
    const BOOT0_ISP_SRAM_RELEASED: u32 = 1 << 0;

    let registers = memory.acquire(soc.boot0_isp_sram_release()?)?;
    runtime::instruction::fence();
    let flags = registers.read::<u32>(0)?;
    registers.write(0, flags | BOOT0_ISP_SRAM_RELEASED)?;
    runtime::instruction::fence();
    Ok(())
}

/// Enables the V821 CCU gate required by the PLMT timer.
pub(crate) fn enable_plmt_clock(soc: AllwinnerV821Soc, memory: &mut MemoryRegistry) -> Result<()> {
    const CLOCK_ENABLE: u32 = 1 << 31;

    let registers = memory.acquire(soc.plmt_clock()?)?;
    let value = u32::from_le(registers.read::<u32>(0)?);
    registers.write(0, (value | CLOCK_ENABLE).to_le())?;
    runtime::instruction::fence();
    Ok(())
}
