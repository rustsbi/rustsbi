//! V821 boot preparation and initialized platform state.

use runtime::{
    Result,
    memory::MemoryRegistry,
    soc::allwinner::v821::{AllwinnerV821Soc, NoncacheableAlias},
};

use super::{control, description::Desc};

/// A V821 SoC whose boot contract and machine-wide PMA are prepared.
pub(crate) struct V821 {
    soc: AllwinnerV821Soc,
    noncacheable_alias: NoncacheableAlias,
}

impl V821 {
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        let noncacheable_alias = control::early_init(description.soc, memory)?;
        Ok(Self {
            soc: description.soc,
            noncacheable_alias,
        })
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
