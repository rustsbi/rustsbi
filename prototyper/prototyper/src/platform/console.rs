//! Inert facts for the selected firmware console.

use alloc::string::{String, ToString};
use core::ops::Range;

use dtoolkit::model::DeviceTreeNode;
use dtoolkit::{Node, Property};

use super::dt::{enabled, node_at_path, reg_at_path};
use super::facts::DiscoverError;

const UART16550_U8: [&str; 1] = ["ns16550a"];
const UART16550_U32: [&str; 1] = ["snps,dw-apb-uart"];
const UART_AXI_LITE: [&str; 1] = ["xlnx,xps-uartlite-1.00.a"];
const UART_BFLB: [&str; 1] = ["bflb,bl808-uart"];
const UART_SIFIVE: [&str; 1] = ["sifive,uart0"];
const UART_PL011: [&str; 1] = ["pl011"];

/// Inert facts for the selected firmware console node.
pub struct Console {
    /// Exact node identity selected by `chosen.stdout-path`.
    pub path: String,
    /// Checked physical register range; this value grants no MMIO authority.
    pub range: Range<usize>,
    /// Register convention selected from the compatible binding.
    pub kind: ConsoleKind,
}

/// Console register conventions retained by platform discovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConsoleKind {
    /// Byte-stride 16550-compatible UART.
    Uart16550U8,
    /// Word-stride 16550-compatible UART.
    Uart16550U32,
    /// Xilinx AXI-lite UART.
    UartAxiLite,
    /// Bouffalo Lab UART.
    UartBflb,
    /// SiFive UART.
    UartSifive,
    /// Arm PL011 UART.
    UartPl011,
}

pub(super) fn discover(root: &DeviceTreeNode) -> Result<Option<Console>, DiscoverError> {
    let Some(chosen) = root.child("chosen") else {
        return Ok(None);
    };
    let Some(stdout) = chosen
        .property("stdout-path")
        .or_else(|| chosen.property("linux,stdout-path"))
        .and_then(|property| property.as_str().ok())
    else {
        return Ok(None);
    };
    let selected = stdout.split(':').next().unwrap_or_default();
    let selected = resolve_alias(root, selected).ok_or(DiscoverError::DeviceTree)?;
    let node = node_at_path(root, selected).ok_or(DiscoverError::DeviceTree)?;
    if !enabled(node) {
        return Ok(None);
    }
    let kind = kind(node).ok_or(DiscoverError::UnsupportedDevice)?;
    Ok(Some(Console {
        path: selected.to_string(),
        range: reg_at_path(root, selected).map_err(|_| DiscoverError::DeviceRange)?,
        kind,
    }))
}

pub(crate) fn install(console: &Console) -> Result<machine::Console, machine::memory::IoMemError> {
    match console.kind {
        ConsoleKind::Uart16550U8 => {
            uart_16550::install(console.range.clone(), uart_16550::Access::Byte)
        }
        ConsoleKind::Uart16550U32 => {
            uart_16550::install(console.range.clone(), uart_16550::Access::Word)
        }
        ConsoleKind::UartAxiLite => uartlite::install(console.range.clone()),
        ConsoleKind::UartBflb => uart_bl808::install(console.range.clone()),
        ConsoleKind::UartSifive => uart_sifive::install(console.range.clone()),
        ConsoleKind::UartPl011 => uart_pl011::install(console.range.clone()),
    }
}

fn resolve_alias<'a>(root: &'a DeviceTreeNode, selected: &'a str) -> Option<&'a str> {
    if selected.starts_with('/') {
        return Some(selected);
    }
    root.child("aliases")?.property(selected)?.as_str().ok()
}

fn kind(node: &DeviceTreeNode) -> Option<ConsoleKind> {
    node.property("compatible")?
        .as_str_list()
        .find_map(|compatible| {
            if UART16550_U8.contains(&compatible) {
                Some(ConsoleKind::Uart16550U8)
            } else if UART16550_U32.contains(&compatible) {
                Some(ConsoleKind::Uart16550U32)
            } else if UART_AXI_LITE.contains(&compatible) {
                Some(ConsoleKind::UartAxiLite)
            } else if UART_BFLB.contains(&compatible) {
                Some(ConsoleKind::UartBflb)
            } else if UART_SIFIVE.contains(&compatible) {
                Some(ConsoleKind::UartSifive)
            } else if UART_PL011.contains(&compatible) {
                Some(ConsoleKind::UartPl011)
            } else {
                None
            }
        })
}
