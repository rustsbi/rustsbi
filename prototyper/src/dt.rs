use serde::Deserialize;
use serde_device_tree::{
    buildin::{Node, NodeSeq, Reg, StrSeq},
    Dtb, DtbPtr,
};

use core::ops::Range;

/// Root device tree structure containing system information.
#[derive(Deserialize)]
pub struct Tree<'a> {
    /// Optional model name string.
    pub model: Option<StrSeq<'a>>,
    /// Chosen node containing boot parameters.
    pub chosen: Chosen<'a>,
    /// Memory information.
    pub memory: NodeSeq<'a>,
    /// CPU information.
    pub cpus: Cpus<'a>,
}

/// Chosen node containing boot parameters.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Chosen<'a> {
    /// Path to stdout device.
    pub stdout_path: StrSeq<'a>,
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

pub fn get_compatible_and_range<'de>(node: &Node) -> Option<(StrSeq<'de>, Range<usize>)> {
    let compatible = node
        .get_prop("compatible")
        .map(|prop_item| prop_item.deserialize::<StrSeq<'de>>());
    let regs = node
        .get_prop("reg")
        .map(|prop_item| {
            let reg = prop_item.deserialize::<serde_device_tree::buildin::Reg>();
            if let Some(range) = reg.iter().next() {
                return Some(range);
            }
            None
        })
        .map_or_else(|| None, |v| v);
    if let Some(compatible) = compatible {
        if let Some(regs) = regs {
            Some((compatible, regs.0))
        } else {
            None
        }
    } else {
        None
    }
}
