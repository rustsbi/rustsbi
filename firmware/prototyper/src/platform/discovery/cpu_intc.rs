//! Device-tree CPU-local interrupt-controller references.
//!
//! A `riscv,cpu-intc` node is the DT endpoint for a hart's local interrupt
//! sources. Interrupt controllers such as IMSIC, MSWI, and MTIMER reference
//! these endpoints from their `interrupts-extended` properties.

use runtime::{FdtNode, node_is_enabled};

use crate::devicetree::u32_property;

/// The identity and availability of one CPU-local interrupt-controller node.
///
/// `phandle` identifies the DT endpoint and `hart_id` identifies its parent
/// CPU. Disabled endpoints remain in this list so controllers can preserve
/// their physical per-hart register positions.
#[derive(Clone, Copy)]
pub(super) struct CpuInterruptController {
    phandle: u32,
    hart_id: usize,
    /// Whether this CPU-local interrupt-controller node is enabled.
    ///
    /// CPU availability is represented separately by whether `hart_id`
    /// resolves through Runtime's published hart topology.
    enabled: bool,
}

impl CpuInterruptController {
    /// Parses one child of a CPU node that represents a local interrupt
    /// controller.
    pub(super) fn from_node(node: FdtNode<'_, '_>, hart_id: usize) -> Option<Self> {
        if node.name.split('@').next() != Some("interrupt-controller")
            || !node.compatible().is_some_and(|compatibles| {
                compatibles
                    .all()
                    .any(|device_id| device_id == "riscv,cpu-intc")
            })
        {
            return None;
        }
        Some(Self {
            phandle: phandle(node)?,
            hart_id,
            enabled: node_is_enabled(node),
        })
    }

    pub(super) const fn phandle(self) -> u32 {
        self.phandle
    }

    pub(super) const fn hart_id(self) -> usize {
        self.hart_id
    }

    pub(super) const fn enabled(self) -> bool {
        self.enabled
    }
}

fn phandle(node: FdtNode<'_, '_>) -> Option<u32> {
    u32_property(node, "phandle").or_else(|| u32_property(node, "linux,phandle"))
}
