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
use runtime::{
    Error, FdtNode, PlatformView, Result, console::ConsoleDevice, memory::MemoryRegistry,
};

/// A UART description pending MMIO acquisition.
pub(crate) enum ConsoleDesc {
    Uart16550U8(uart16550::DescU8),
    Uart16550U32(uart16550::DescU32),
    AxiLite(axi_lite::Desc),
    Bl808(bl808::Desc),
    Sifive(sifive::Desc),
    Pl011(pl011::Desc),
    Xscale(xscale::Desc),
}

impl ConsoleDesc {
    pub(crate) fn init(self, memory: &mut MemoryRegistry) -> Result<Box<dyn ConsoleDevice>> {
        Ok(match self {
            Self::Uart16550U8(desc) => Box::new(
                uart16550::Device::<uart16550::U8RegisterAccess>::init(desc, memory)?,
            ),
            Self::Uart16550U32(desc) => Box::new(
                uart16550::Device::<uart16550::U32RegisterAccess>::init(desc, memory)?,
            ),
            Self::AxiLite(desc) => Box::new(axi_lite::Device::init(desc, memory)?),
            Self::Bl808(desc) => Box::new(bl808::Device::init(desc, memory)?),
            Self::Sifive(desc) => Box::new(sifive::Device::init(desc, memory)?),
            Self::Pl011(desc) => Box::new(pl011::Device::init(desc, memory)?),
            Self::Xscale(desc) => Box::new(xscale::Device::init(desc, memory)?),
        })
    }
}

/// Selects the console named by `/chosen/stdout-path`.
///
/// If the chosen console is absent or unsupported, scans for the first
/// supported UART in an enabled subtree. A malformed supported UART returns an error.
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

    let mut selected = None;
    platform.try_for_each_enabled_node(|node, _, _| {
        if selected.is_none() {
            selected = describe(platform, node)?;
        }
        Ok(())
    })?;
    Ok(selected)
}

/// Parses a UART node in its declared `compatible` order.
///
/// Returns [`Error::InvalidArgs`] if a supported binding matches but no
/// parser accepts the node.
pub(crate) fn describe(
    platform: &PlatformView<'_>,
    node: FdtNode<'_, '_>,
) -> Result<Option<ConsoleDesc>> {
    let Some(compatibles) = node.compatible() else {
        return Ok(None);
    };
    let mut matched = false;
    for compatible in compatibles.all() {
        let description = match compatible {
            "ns16550" | "ns16550a" | "snps,dw-apb-uart" | "allwinner,sunxi-uart" => {
                uart16550::describe(platform, node, compatible)
            }
            "xlnx,xps-uartlite-1.00.a" => axi_lite::describe(platform, node),
            "bflb,bl808-uart" => bl808::describe(platform, node),
            "sifive,uart0" => sifive::describe(platform, node),
            "pl011" | "arm,pl011" => pl011::describe(platform, node),
            "intel,xscale-uart" | "spacemit,k1-uart" => xscale::describe(platform, node),
            _ => continue,
        }?;
        if description.is_some() {
            return Ok(description);
        }
        matched = true;
    }
    if matched {
        Err(Error::InvalidArgs)
    } else {
        Ok(None)
    }
}
