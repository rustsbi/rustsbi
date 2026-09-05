#![forbid(unsafe_code)]

use alloc::string::ToString;
use serde::Deserialize;
use serde_device_tree::{
    buildin::{Node, NodeSeq, Reg, StrSeq},
    value::riscv_pmu::{EventToMhpmcounters, EventToMhpmevent, RawEventToMhpcounters},
};

/// Root device tree structure containing system information.
#[derive(Deserialize)]
pub struct Tree<'a> {
    /// Optional model name string.
    pub model: Option<StrSeq<'a>>,
    /// CPU information.
    pub cpus: Cpus<'a>,
}

/// CPU information container.
#[derive(Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Cpus<'a> {
    /// Frequency of the architectural `time` counter, in hertz.
    #[serde(rename = "timebase-frequency")]
    pub timebase_frequency_hz: Option<u32>,
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

#[derive(Deserialize)]
pub struct Pmu<'a> {
    #[serde(rename = "riscv,event-to-mhpmevent")]
    pub event_to_mhpmevent: Option<EventToMhpmevent<'a>>,
    #[serde(rename = "riscv,event-to-mhpmcounters")]
    pub event_to_mhpmcounters: Option<EventToMhpmcounters<'a>>,
    #[serde(rename = "riscv,raw-event-to-mhpmcounters")]
    pub raw_event_to_mhpmcounters: Option<RawEventToMhpcounters<'a>>,
}

pub fn compatible_strings<'de>(node: &Node) -> Option<StrSeq<'de>> {
    node.get_prop("compatible")
        .map(|property| property.deserialize::<StrSeq<'de>>())
}

/// Resolves an absolute path or alias, rejecting a disabled node or ancestor.
pub fn find_enabled_node<'de>(root: &Node<'de>, path: &str) -> Option<Node<'de>> {
    if !runtime::node_is_enabled(root) {
        return None;
    }

    let resolved_path = if path.starts_with('/') {
        path.to_string()
    } else {
        let aliases = root.find("/aliases")?;
        if !runtime::node_is_enabled(&aliases) {
            return None;
        }
        aliases
            .get_prop(path)?
            .deserialize::<StrSeq>()
            .iter()
            .next()?
            .to_string()
    };

    if resolved_path == "/" {
        return Some(root.clone());
    }
    let path = resolved_path.strip_prefix('/')?;
    let mut current_node = root.clone();
    for name in path.split('/') {
        if name.is_empty() {
            return None;
        }
        let child_node = {
            let child = current_node
                .nodes()
                .find(|child| child.get_full_name() == name)?;
            child.deserialize::<Node<'de>>()
        };
        if !runtime::node_is_enabled(&child_node) {
            return None;
        }
        current_node = child_node;
    }
    Some(current_node)
}

/// Visits enabled nodes depth first.
pub fn visit_enabled_nodes<F>(root: &Node, visitor: &mut F)
where
    F: FnMut(&Node),
{
    fn visit_subtree<'de, F>(node: &Node<'de>, visitor: &mut F)
    where
        F: FnMut(&Node<'de>),
    {
        if !runtime::node_is_enabled(node) {
            return;
        }
        visitor(node);
        for child in node.nodes() {
            let child = child.deserialize::<Node<'de>>();
            visit_subtree(&child, visitor);
        }
    }
    visit_subtree(root, visitor);
}
