//! K3 application-processor capability recognized from the Devicetree root.
//!
//! All K3 cores (X100, A100 and RT24) are 64-bit RISC-V cores.
//!
//! # References
//!
//! - [Linux K3 Devicetree](https://github.com/torvalds/linux/blob/1f5d8228ca2c06ee1baa81b40cdba824ed768d95/arch/riscv/boot/dts/spacemit/k3.dtsi):
//!   `spacemit,k3` root compatible and `rv64i` CPUs.
//! - [Vendor K3 CPU Devicetree](https://github.com/spacemit-com/linux-6.18/blob/0bef28bfeb51b20a62d82232de44556df942f4dc/arch/riscv/boot/dts/spacemit/k3-cpus.dtsi):
//!   X100 and A100 CPUs with `riscv,isa-base = "rv64i"`.
//! - [Vendor K3 OpenSBI](https://github.com/spacemit-com/opensbi/blob/742a8aec8662cb60c3ea49bec6dbbadd77ea833d/platform/generic/spacemit/spacemit_k3.c):
//!   platform match for `spacemit,k3`.
//! - [K3 datasheet](https://github.com/spacemit-com/docs-chip/blob/abd7802bc2151fe814b90309fc69982e531b30a3/en/key_stone/k3/k3_docs/k3_ds.md):
//!   X100 and A100 core descriptions, and RT24's RV64GC ISA (section 2.1).

use serde_device_tree::buildin::{Node, StrSeq};

use crate::Result;
use crate::soc::Soc;

/// A K3 AP/X100 capability, distinct from the RT24 real-time subsystem.
///
/// The type carries no MMIO state.
#[derive(Clone, Copy)]
pub struct SpacemitK3Soc {
    _private: (),
}

impl Soc for SpacemitK3Soc {
    fn from_root(root: &Node<'_>) -> Result<Option<Self>> {
        Ok((cfg!(target_arch = "riscv64") && matches_root(root)).then_some(Self { _private: () }))
    }
}

fn matches_root(root: &Node<'_>) -> bool {
    // A generic SpacemiT match does not identify the X100 application cores.
    crate::node_is_enabled(root)
        && root.get_prop("compatible").is_some_and(|property| {
            property
                .deserialize::<StrSeq>()
                .iter()
                .any(|compatible| compatible == "spacemit,k3")
        })
}
