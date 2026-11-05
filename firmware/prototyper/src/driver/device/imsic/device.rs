//! IMSIC interrupt files and the firmware's message sender.

use alloc::{boxed::Box, vec::Vec};
use runtime::{
    Error, Result,
    hart::HartId,
    ipi::{External, Iid, IpiDevice, imsic},
    memory::{MemoryRegistry, MmioRegion},
};

use super::description::Desc;

/// MMIO offset of `seteipnum_le` in every interrupt file, as specified by AIA 1.0.
const SET_EIPNUM_LE: usize = 0x0;

/// A firmware IPI sender backed by IMSIC interrupt files.
pub(super) struct ImsicIpi {
    files: Box<[MmioRegion]>,
    iid: Iid,
}

impl ImsicIpi {
    /// Initializes the machine IMSIC CSRs and acquires one file per hart.
    pub(super) fn init(description: Desc, memory: &mut MemoryRegistry) -> Result<Self> {
        // Probe before acquiring any register windows.
        // Another IPI device remains usable if this hart lacks the machine
        // IMSIC CSRs.
        imsic::init(description.ipi_iid, description.num_iids)?;
        let files = description
            .hart_files
            .into_iter()
            .map(|range| memory.acquire(range))
            .collect::<Result<Vec<_>>>()?
            .into_boxed_slice();
        Ok(Self {
            files,
            iid: description.ipi_iid,
        })
    }
}

impl IpiDevice<External> for ImsicIpi {
    fn send_ipi(&self, hart: HartId) -> Result<()> {
        let identity = u32::from(self.iid.number());
        self.files[hart.index()]
            .write(SET_EIPNUM_LE, identity)
            .map_err(|_| Error::IoError)
    }
}
