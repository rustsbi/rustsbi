//! Allwinner F101 fixed-register descriptions.
//!
//! The [F101 OpenSBI platform] selects this watchdog using the SoC root
//! compatible; existing Neko device trees do not describe it as a child node.
//!
//! [F101 OpenSBI platform]: https://github.com/YuzukiHD/opensbi/blob/08cbbe0bf6d2aaf0e8dbef8c548f4befdf3b7a7e/platform/generic/allwinner/sun252i-f101.c

use serde_device_tree::buildin::{Node, StrSeq};

use crate::memory::{DeviceRegisterRange, PhysAddr, PhysAddrRange};
use crate::{Result, node_is_enabled};

const F101_COMPATIBLE: &str = "allwinner,sun252i-f101";
const WATCHDOG_BASE: PhysAddr = PhysAddr::new(0x0601_1000);
// Includes WDT_CFG at 0x14 and the 32-bit WDT_MODE at 0x18.
const WATCHDOG_SPAN: usize = 0x1c;

pub(crate) fn watchdog_registers(root: &Node<'_>) -> Result<Option<DeviceRegisterRange>> {
    if !node_is_enabled(root)
        || !root.get_prop("compatible").is_some_and(|property| {
            property
                .deserialize::<StrSeq>()
                .iter()
                .any(|value| value == F101_COMPATIBLE)
        })
    {
        return Ok(None);
    }
    PhysAddrRange::from_start_len(WATCHDOG_BASE, WATCHDOG_SPAN)
        .map(DeviceRegisterRange::from_description)
        .map(Some)
}
