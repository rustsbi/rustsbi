//! V861 C907 power and reset windows from the [V861 OpenSBI platform].
//!
//! [V861 OpenSBI platform]: https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/allwinner/sun252i-v861.c

use crate::memory::{DeviceRegisterRange, PhysAddr, PhysAddrRange};
use crate::{Result, node_is_enabled};
use serde_device_tree::buildin::{Node, StrSeq};

/// Fixed register descriptions authorized by a V861 root compatible.
#[derive(Clone, Copy)]
pub struct AllwinnerV861Registers {
    _private: (),
}

impl AllwinnerV861Registers {
    pub(crate) fn from_root(root: &Node<'_>) -> Option<Self> {
        (node_is_enabled(root)
            && root.get_prop("compatible").is_some_and(|p| {
                p.deserialize::<StrSeq>()
                    .iter()
                    .any(|s| matches!(s, "allwinner,sun252i-v861" | "allwinner,sun252iw1p1"))
            }))
        .then_some(Self { _private: () })
    }

    /// C907 configuration bus gate/reset register.
    pub fn clock_gate(self) -> Result<DeviceRegisterRange> {
        range(0x0200_150c, 4)
    }
    /// Each C907 reset vector and execution-mode register.
    pub fn reset_vector(self, hart: usize) -> Result<DeviceRegisterRange> {
        if hart >= 2 {
            return Err(crate::Error::InvalidArgs);
        }
        range(0x0800_8100 + 0x80 * hart, 12)
    }
    /// The two per-core software power controls.
    pub fn power_control(self, hart: usize) -> Result<DeviceRegisterRange> {
        if hart >= 2 {
            return Err(crate::Error::InvalidArgs);
        }
        range(0x0800_0100 + 0x1000 * hart, 8)
    }
}

fn range(start: usize, len: usize) -> Result<DeviceRegisterRange> {
    PhysAddrRange::from_start_len(PhysAddr::new(start), len)
        .map(DeviceRegisterRange::from_description)
}
