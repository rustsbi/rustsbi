use alloc::vec::Vec;
use serde::Deserialize;
use serde_device_tree::{
    Dtb, DtbPtr,
    buildin::{Node, NodeSeq, Reg, StrSeq},
    value::riscv_pmu::{EventToMhpmcounters, EventToMhpmevent, RawEventToMhpcounters},
};

use core::ops::Range;

/// Root device tree structure containing system information.
#[derive(Deserialize)]
pub struct Tree<'a> {
    /// Optional model name string.
    pub model: Option<StrSeq<'a>>,
    /// Memory information.
    pub memory: NodeSeq<'a>,
    /// CPU information.
    pub cpus: Cpus<'a>,
}

/// CPU information container.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Cpus<'a> {
    /// Sequence of CPU nodes.
    pub cpu: NodeSeq<'a>,
}

/// Individual CPU node information.
#[derive(Deserialize, Debug)]
pub struct Cpu<'a> {
    /// RISC-V ISA extensions supported by this CPU.
    #[serde(rename = "riscv,isa-extensions")]
    pub isa_extensions: Option<StrSeq<'a>>,
    #[serde(rename = "riscv,isa")]
    pub isa: Option<StrSeq<'a>>,
    /// CPU register information.
    pub reg: Reg<'a>,
}

/// Generic device node information.
#[allow(unused)]
#[derive(Deserialize, Debug)]
pub struct Device<'a> {
    /// Device register information.
    pub reg: Reg<'a>,
}

/// Memory range.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Memory<'a> {
    pub reg: Reg<'a>,
}

#[derive(Deserialize)]
pub struct Pmu<'a> {
    #[serde(rename = "riscv,event-to-mhpmevent")]
    pub event_to_mhpmevent: Option<EventToMhpmevent<'a>>,
    #[serde(rename = "riscv,event-to-mhpmcounters")]
    pub event_to_mhpmcounters: Option<EventToMhpmcounters<'a>>,
    #[serde(rename = "riscv,raw-event-to-mhpmcounters")]
    pub raw_event_to_mhpmcounters: Option<RawEventToMhpcounters<'a>>,
}

/// Errors that can occur during device tree parsing.
pub enum ParseDeviceTreeError {
    /// Invalid device tree format.
    Format,
}

pub fn parse_device_tree(opaque: usize) -> Result<Dtb, ParseDeviceTreeError> {
    let Ok(ptr) = DtbPtr::from_raw(opaque as *mut _) else {
        return Err(ParseDeviceTreeError::Format);
    };
    let dtb = Dtb::from(ptr);
    Ok(dtb)
}

pub fn get_compatible_and_ranges<'de>(node: &Node) -> Option<(StrSeq<'de>, Vec<Range<usize>>)> {
    let compatible = node
        .get_prop("compatible")
        .map(|prop_item| prop_item.deserialize::<StrSeq<'de>>());
    let regs = node.get_prop("reg").map(|prop_item| {
        let reg = prop_item.deserialize::<serde_device_tree::buildin::Reg>();
        reg.iter().map(|range| range.0).collect::<Vec<_>>()
    });
    if let Some(compatible) = compatible {
        if let Some(regs) = regs {
            if regs.is_empty() {
                None
            } else {
                Some((compatible, regs))
            }
        } else {
            None
        }
    } else {
        None
    }
}

pub fn get_compatible_and_range<'de>(node: &Node) -> Option<(StrSeq<'de>, Range<usize>)> {
    let (compatible, regs) = get_compatible_and_ranges(node)?;
    regs.into_iter().next().map(|range| (compatible, range))
}

pub fn get_compatible<'de>(node: &Node) -> Option<StrSeq<'de>> {
    let compatible = node
        .get_prop("compatible")
        .map(|prop_item| prop_item.deserialize::<StrSeq<'de>>());
    if let Some(compatible) = compatible {
        Some(compatible)
    } else {
        None
    }
}

/// Depth-first traversal of the device tree that also yields each node's
/// parent, so callers can resolve parent-child relationships (e.g. a child
/// device's parent bus) that `Node` does not expose directly.
pub fn search_with_parent<F>(root: &Node, func: &mut F)
where
    F: FnMut(&Node, Option<&Node>),
{
    fn dfs<'de, F>(node: &Node<'de>, parent: Option<&Node<'de>>, func: &mut F)
    where
        F: FnMut(&Node<'de>, Option<&Node<'de>>),
    {
        func(node, parent);
        for child in node.nodes() {
            let child = child.deserialize::<Node<'de>>();
            dfs(&child, Some(node), func);
        }
    }
    dfs(root, None, func);
}
