//! Byte-oriented console backends.
//!
//! # References
//!
//! - Devicetree bindings: [8250], [DesignWare APB UART], [AXI UART Lite],
//!   and [SiFive UART] describe the properties used in discovery.
//!
//! [8250]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/serial/8250.yaml
//! [DesignWare APB UART]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/serial/snps-dw-apb-uart.yaml
//! [AXI UART Lite]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/serial/xlnx%2Copb-uartlite.yaml
//! [SiFive UART]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/serial/sifive-serial.yaml

mod axi_lite;
mod bl808;
mod pl011;
mod sifive;
mod uart16550;
mod xscale;

use alloc::boxed::Box;
use runtime::{Error, FdtNode, PlatformView, Result, console::ConsoleDevice, node_is_enabled};

use super::{Description, match_desc};

/// A parsed console device ready to acquire registers and initialize a UART.
pub(crate) type ConsoleDesc = Box<dyn Description<Device = Box<dyn ConsoleDevice>>>;

/// Selects `/chosen/stdout-path`, falling back to the first supported console.
pub(crate) fn select(platform: &PlatformView<'_>) -> Result<Option<ConsoleDesc>> {
    if let Some(chosen) = platform.find_enabled_node("/chosen")
        && let Some(stdout_path) = chosen
            .property("stdout-path")
            .and_then(|property| property.as_str())
            .and_then(|path| path.split(':').next())
        && let Some(node) = platform.find_enabled_node(stdout_path)
        && let Some(console) = describe(platform, node)?
    {
        return Ok(Some(console));
    }

    for node in platform.fdt().all_nodes() {
        if !node_is_enabled(node) {
            continue;
        }
        if let Some(console) = describe(platform, node)? {
            return Ok(Some(console));
        }
    }
    Ok(None)
}

/// Parses one console node into a concrete UART description.
///
/// Compatible strings are tried in device-tree order.
/// Within one compatible, the historical driver fallback order is preserved.
pub(crate) fn describe(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> Result<Option<ConsoleDesc>> {
    let Some(compatibles) = node.compatible() else {
        return Ok(None);
    };
    match_desc!(for compatible in compatibles.all(), Error::InvalidArgs; {
        "ns16550" | "ns16550a" => [
            uart16550::describe_u8(platform, node),
            uart16550::describe_u32(platform, node, compatible),
        ],
        "snps,dw-apb-uart" | "allwinner,sunxi-uart" => [
            uart16550::describe_u32(platform, node, compatible),
        ],
        "xlnx,xps-uartlite-1.00.a" => [axi_lite::describe(platform, node)],
        "bflb,bl808-uart" => [bl808::describe(platform, node)],
        "sifive,uart0" => [sifive::describe(platform, node)],
        "pl011" | "arm,pl011" => [pl011::describe(platform, node)],
        "intel,xscale-uart" | "spacemit,k1-uart" => [xscale::describe(platform, node)],
    })
}
