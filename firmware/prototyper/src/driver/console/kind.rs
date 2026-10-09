//! Console device selection from FDT identity and register layout.
//!
//! Compatible strings and 8250 `reg-shift`/`reg-io-width` semantics follow
//! the pinned Linux Devicetree bindings for [8250], [DesignWare APB UART],
//! [AXI UART Lite], and [SiFive UART]. Hardware register layouts remain
//! documented by the individual drivers.
//!
//! [8250]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/serial/8250.yaml
//! [DesignWare APB UART]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/serial/snps-dw-apb-uart.yaml
//! [AXI UART Lite]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/serial/xlnx%2Copb-uartlite.yaml
//! [SiFive UART]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/serial/sifive-serial.yaml

/// The console device kinds the firmware can drive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConsoleKind {
    Uart16550U8,
    Uart16550U32,
    AxiLite,
    Bl808,
    SiFive,
    Pl011,
    XScale,
}

const UART_16550_COMPATIBLES: [&str; 2] = ["ns16550", "ns16550a"];
const UART_16550_U32_COMPATIBLES: [&str; 2] = ["snps,dw-apb-uart", "allwinner,sunxi-uart"];
const UART_AXI_LITE_COMPATIBLES: [&str; 1] = ["xlnx,xps-uartlite-1.00.a"];
const UART_BFLB_COMPATIBLES: [&str; 1] = ["bflb,bl808-uart"];
const UART_SIFIVE_COMPATIBLES: [&str; 1] = ["sifive,uart0"];
const UART_PL011_COMPATIBLES: [&str; 2] = ["pl011", "arm,pl011"];
const UART_XSCALE_COMPATIBLES: [&str; 2] = ["intel,xscale-uart", "spacemit,k1-uart"];

impl ConsoleKind {
    /// Selects the first supported identity and layout in compatible order.
    /// Unknown families return `None`; a known family with no usable layout
    /// returns an error after trying the remaining compatibles.
    pub(crate) fn from_fdt<'a>(
        compatibles: impl IntoIterator<Item = &'a str>,
        register_shift: Option<u32>,
        register_width: Option<u32>,
    ) -> runtime::Result<Option<Self>> {
        let u8_layout = register_shift.unwrap_or(0) == 0 && register_width.unwrap_or(1) == 1;
        let u32_layout = register_shift == Some(2) && register_width == Some(4);
        let mut unsupported_layout = false;
        for compatible in compatibles {
            let kind = if UART_16550_COMPATIBLES.contains(&compatible) {
                if u8_layout {
                    Some(Self::Uart16550U8)
                } else if u32_layout {
                    Some(Self::Uart16550U32)
                } else {
                    unsupported_layout = true;
                    None
                }
            } else if UART_16550_U32_COMPATIBLES.contains(&compatible) {
                // Preserve identity-based selection for older device trees
                // that omit the register layout properties.
                Some(Self::Uart16550U32)
            } else if UART_AXI_LITE_COMPATIBLES.contains(&compatible) {
                Some(Self::AxiLite)
            } else if UART_BFLB_COMPATIBLES.contains(&compatible) {
                Some(Self::Bl808)
            } else if UART_SIFIVE_COMPATIBLES.contains(&compatible) {
                Some(Self::SiFive)
            } else if UART_PL011_COMPATIBLES.contains(&compatible) {
                Some(Self::Pl011)
            } else if UART_XSCALE_COMPATIBLES.contains(&compatible) {
                Some(Self::XScale)
            } else {
                None
            };
            if let Some(kind) = kind {
                return Ok(Some(kind));
            }
        }
        if unsupported_layout {
            Err(runtime::Error::InvalidArgs)
        } else {
            Ok(None)
        }
    }

    /// Returns the device name used in boot logs.
    pub(crate) fn name(self) -> &'static str {
        match self {
            ConsoleKind::Uart16550U8 => "Uart16550U8",
            ConsoleKind::Uart16550U32 => "Uart16550U32",
            ConsoleKind::AxiLite => "UartAxiLite",
            ConsoleKind::Bl808 => "UartBl808",
            ConsoleKind::SiFive => "UartSiFive",
            ConsoleKind::Pl011 => "UartPl011",
            ConsoleKind::XScale => "UartXScale",
        }
    }
}
