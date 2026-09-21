//! Fixed V821 resource facts authorized by the Devicetree root.
//!
//! # References
//!
//! - Vendor platform source: [V821 SPL board setup](https://github.com/sam-yangjj/tina-v821-v1.3-brandy/blob/34809037526678ccda1720cb4b4ec7ee32272c38/brandy-2.0/spl/board/sun300iw1p1/board.c)
//!   — A27L2 machine-timer clock gate used during boot.

use core::mem::size_of;

use fdt::node::FdtNode;

use crate::Result;
use crate::memory::{DeviceRegisterRange, PhysAddr, PhysAddrRange};
use crate::soc::Soc;

const V821_COMPATIBLE: &str = "allwinner,v821";
const CCU_BASE: usize = 0x4200_1000;
const PLMT_CLOCK_CONTROL_OFFSET: usize = 0x10;

/// A V821 root-compatible capability.
///
/// This type contains no register state. It proves that Platform Description
/// selected V821 and authorizes callers to derive its fixed resources and
/// custom-CSR interface.
#[derive(Clone, Copy)]
pub struct AllwinnerV821Soc {
    _private: (),
}

impl Soc for AllwinnerV821Soc {
    fn from_root(root: FdtNode<'_, '_>) -> Result<Option<Self>> {
        Ok((crate::node_is_enabled(root)
            && root
                .compatible()
                .is_some_and(|values| values.all().any(|value| value == V821_COMPATIBLE)))
        .then_some(Self { _private: () }))
    }
}

impl AllwinnerV821Soc {
    /// PLMT timer clock gate in the V821 CCU register map.
    pub fn plmt_clock(self) -> Result<DeviceRegisterRange> {
        PhysAddrRange::from_start_len(
            PhysAddr::new(CCU_BASE + PLMT_CLOCK_CONTROL_OFFSET),
            size_of::<u32>(),
        )
        .map(DeviceRegisterRange::from_description)
    }
}
