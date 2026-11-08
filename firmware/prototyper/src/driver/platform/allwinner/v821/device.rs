//! V821 boot preparation and initialized platform state.

use csr::machine::{Identifier, VendorId};
use runtime::{
    Error, Result,
    hart::HartId,
    memory::MemoryRegistry,
    soc::allwinner::v821::{AllwinnerV821Soc, NoncacheableAlias},
};

use super::{control::release_boot0_isp_sram, description::Desc};

/// A V821 SoC whose boot contract and machine-wide PMA are prepared.
pub(crate) struct V821 {
    soc: AllwinnerV821Soc,
    noncacheable_alias: NoncacheableAlias,
}

impl V821 {
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let hart = HartId::current().map_err(|_| Error::InvalidArgs)?;
        const BOOT_HART: usize = 0;
        const ANDES_VENDOR_ID: Identifier = Identifier::from_bits(0x31e);
        if hart.as_usize() != BOOT_HART
            || HartId::count() != 1
            || VendorId::read() != ANDES_VENDOR_ID
        {
            return Err(Error::InvalidArgs);
        }
        let noncacheable_alias = description.soc.csr().initialize_noncacheable_alias();
        let platform = Self {
            soc: description.soc,
            noncacheable_alias,
        };
        release_boot0_isp_sram(platform.soc, memory)?;
        Ok(platform)
    }

    /// Returns the offset of the noncacheable alias established during preparation.
    pub(crate) const fn noncacheable_alias_offset(&self) -> u64 {
        self.noncacheable_alias.offset()
    }

    /// Returns the SoC capability required by V821 firmware drivers.
    pub(crate) const fn soc(&self) -> AllwinnerV821Soc {
        self.soc
    }
}
