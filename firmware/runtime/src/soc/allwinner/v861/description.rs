//! Fixed V861 resource facts authorized by the Devicetree root.
//!
//! # References
//!
//! - Platform source: [V861 OpenSBI platform](https://github.com/YuzukiHD/opensbi/blob/c1ea219a901ff309e88e99c4b4e66ef9a55548de/platform/generic/allwinner/sun252i-v861.c)
//!   — C907 hart count and fixed register locations.

use core::mem::size_of;

use crate::Result;
use crate::memory::{DeviceRegisterRange, PhysAddr, PhysAddrRange};
use crate::soc::Soc;
use serde_device_tree::buildin::{Node, StrSeq};

/// Number of C907 harts controlled by the V861 power controller.
pub const C907_HART_COUNT: usize = 2;

const COMPATIBLES: [&str; 2] = ["allwinner,sun252i-v861", "allwinner,sun252iw1p1"];
const CCU_BASE: usize = 0x0200_1000;
const C907_CONFIG_BUS_GATE_RESET_OFFSET: usize = 0x50c;

const PMC_BASE: usize = 0x0800_0000;
const PMC_HART_STRIDE: usize = 0x1000;
const SOFTWARE_MODE_CONTROL_0_OFFSET: usize = 0x100;
const SOFTWARE_MODE_CONTROL_1_OFFSET: usize = 0x104;
const POWER_CONTROL_SPAN: usize =
    SOFTWARE_MODE_CONTROL_1_OFFSET - SOFTWARE_MODE_CONTROL_0_OFFSET + size_of::<u32>();

const RISCV_CONFIG_BASE: usize = 0x0800_8000;
const RISCV_CONFIG_HART_STRIDE: usize = 0x80;
const RESET_ENTRY_LOW_OFFSET: usize = 0x100;
const CORE_CONFIG_OFFSET: usize = 0x108;
const RESET_VECTOR_SPAN: usize = CORE_CONFIG_OFFSET - RESET_ENTRY_LOW_OFFSET + size_of::<u32>();

/// A V861 root-compatible capability.
///
/// The type carries no MMIO state. It authorizes callers to derive the fixed
/// C907 register windows and use the V861-specific custom CSRs.
#[derive(Clone, Copy)]
pub struct AllwinnerV861Soc {
    _private: (),
}

impl Soc for AllwinnerV861Soc {
    fn from_root(root: &Node<'_>) -> Result<Option<Self>> {
        Ok((crate::node_is_enabled(root)
            && root.get_prop("compatible").is_some_and(|property| {
                property
                    .deserialize::<StrSeq>()
                    .iter()
                    .any(|compatible| COMPATIBLES.contains(&compatible))
            }))
        .then_some(Self { _private: () }))
    }
}

impl AllwinnerV861Soc {
    /// CCU bus-gate/reset word controlling access to the C907 configuration block.
    pub fn c907_clock_control(self) -> Result<DeviceRegisterRange> {
        range(
            CCU_BASE + C907_CONFIG_BUS_GATE_RESET_OFFSET,
            size_of::<u32>(),
        )
    }

    /// Per-hart C907 reset-entry and execution-mode register window.
    ///
    /// The window contains the reset-entry low word, reset-entry high word,
    /// and core-configuration word. Consecutive harts are separated by the
    /// V861 RISCV_CFG stride.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::InvalidArgs`] when `hart` is outside the C907
    /// hart range.
    pub fn hart_reset_vector(self, hart: usize) -> Result<DeviceRegisterRange> {
        if hart >= C907_HART_COUNT {
            return Err(crate::Error::InvalidArgs);
        }
        range(
            RISCV_CONFIG_BASE + RESET_ENTRY_LOW_OFFSET + RISCV_CONFIG_HART_STRIDE * hart,
            RESET_VECTOR_SPAN,
        )
    }

    /// Per-hart PMC software-mode control window used to power on a C907.
    ///
    /// The two words select the power-clamp mode and power-switch mode.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::InvalidArgs`] when `hart` is outside the C907
    /// hart range.
    pub fn hart_power_control(self, hart: usize) -> Result<DeviceRegisterRange> {
        if hart >= C907_HART_COUNT {
            return Err(crate::Error::InvalidArgs);
        }
        range(
            PMC_BASE + PMC_HART_STRIDE * hart + SOFTWARE_MODE_CONTROL_0_OFFSET,
            POWER_CONTROL_SPAN,
        )
    }
}

fn range(start: usize, len: usize) -> Result<DeviceRegisterRange> {
    PhysAddrRange::from_start_len(PhysAddr::new(start), len)
        .map(DeviceRegisterRange::from_description)
}
