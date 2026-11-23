//! V821 boot handshake and platform-specific devices.

#![forbid(unsafe_code)]

use csr::machine::{Identifier, VendorId};
use runtime::{
    Error, Result,
    hart::HartId,
    memory::MemoryRegistry,
    soc::allwinner::v821::{AllwinnerV821Soc, NoncacheableAlias},
};

/// Programs the V821 PMA before releasing the loader's ISP SRAM.
///
/// Returns the noncacheable alias so PMP can protect firmware through both mappings.
pub(super) fn early_init(
    soc: AllwinnerV821Soc,
    memory: &mut MemoryRegistry,
) -> Result<NoncacheableAlias> {
    const BOOT_HART: usize = 0;
    const ANDES_VENDOR_ID: Identifier = Identifier::from_bits(0x31e);

    let hart = HartId::current().map_err(|_| Error::InvalidArgs)?;
    if hart.as_usize() != BOOT_HART || HartId::count() != 1 || VendorId::read() != ANDES_VENDOR_ID {
        return Err(Error::InvalidArgs);
    }
    let alias = soc.csr().initialize_noncacheable_alias();
    release_boot0_isp_sram(soc, memory)?;
    Ok(alias)
}

/// Lets the RTOS ISP driver reclaim boot0's SRAM after firmware moves to DRAM.
fn release_boot0_isp_sram(soc: AllwinnerV821Soc, memory: &mut MemoryRegistry) -> Result<()> {
    const BOOT0_ISP_SRAM_RELEASED: u32 = 1 << 0;

    let registers = memory.acquire(soc.boot0_isp_sram_release()?)?;
    runtime::instruction::fence();
    let flags = registers.read::<u32>(0)?;
    registers.write(0, flags | BOOT0_ISP_SRAM_RELEASED)?;
    runtime::instruction::fence();
    Ok(())
}
