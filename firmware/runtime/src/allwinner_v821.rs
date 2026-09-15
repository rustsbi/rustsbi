//! V821 machine-mode resources identified by the BSP root compatible.

use crate::memory::{DeviceRegisterRange, PhysAddr, PhysAddrRange};
use crate::{Result, node_is_enabled};
use serde_device_tree::buildin::{Node, StrSeq};

/// SoC facts authorized by a V821 platform description.
#[derive(Clone, Copy)]
pub struct AllwinnerV821Registers {
    _private: (),
}

impl AllwinnerV821Registers {
    pub(crate) fn from_root(root: &Node<'_>) -> Option<Self> {
        (node_is_enabled(root)
            && root.get_prop("compatible").is_some_and(|p| {
                p.deserialize::<StrSeq>()
                    .iter()
                    .any(|s| s == "allwinner,v821")
            }))
        .then_some(Self { _private: () })
    }

    /// PLMT clock control from the V821 CCU register map.
    pub fn plmt_clock(self) -> Result<DeviceRegisterRange> {
        PhysAddrRange::from_start_len(PhysAddr::new(0x4200_1010), 4)
            .map(DeviceRegisterRange::from_description)
    }

    /// The A27L2 Sv32 noncacheable physical alias used by the V821 BSP.
    pub const fn noncacheable_offset(self) -> u64 {
        0x1_0000_0000
    }
}
