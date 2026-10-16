//! SBI remote fence requests and their owned per-hart queues.

#![forbid(unsafe_code)]

mod abi;
mod delivery;
mod queue;
mod range;

pub(crate) use delivery::rfence_poll;
use runtime::{Result, hart::HartLocal};

use range::AddressRange;

/// A remote fence request carrying only the operands used by its instruction.
#[derive(Clone, Copy, Debug)]
enum Request {
    FenceI,
    SFenceVma(AddressRange),
    SFenceVmaAsid {
        range: AddressRange,
        asid: usize,
    },
    #[cfg(feature = "hypervisor")]
    HFenceGvma(AddressRange),
    #[cfg(feature = "hypervisor")]
    HFenceGvmaVmid {
        range: AddressRange,
        vmid: usize,
    },
    #[cfg(feature = "hypervisor")]
    HFenceVvma {
        range: AddressRange,
    },
    #[cfg(feature = "hypervisor")]
    HFenceVvmaAsid {
        range: AddressRange,
        asid: usize,
    },
}

#[cfg(feature = "hypervisor")]
impl Request {
    fn is_guest(self) -> bool {
        matches!(
            self,
            Self::HFenceGvma(_)
                | Self::HFenceGvmaVmid { .. }
                | Self::HFenceVvma { .. }
                | Self::HFenceVvmaAsid { .. }
        )
    }
}

/// Implementation of RISC-V remote fence operations.
pub(crate) struct SbiRFence {
    harts: HartLocal<queue::RFenceCell>,
}

impl SbiRFence {
    pub(crate) fn new() -> Result<Self> {
        let harts = HartLocal::new();
        harts.init_all(|_| queue::RFenceCell::new())?;
        Ok(Self { harts })
    }
}
