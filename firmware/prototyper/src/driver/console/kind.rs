//! Console backend selection from FDT identity and register layout.
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
    SpacemitK1,
}

const UART_16550A_COMPATIBLES: [&str; 1] = ["ns16550a"];
const UART_16550_U32_COMPATIBLES: [&str; 1] = ["snps,dw-apb-uart"];
const UART_AXI_LITE_COMPATIBLES: [&str; 1] = ["xlnx,xps-uartlite-1.00.a"];
const UART_BFLB_COMPATIBLES: [&str; 1] = ["bflb,bl808-uart"];
const UART_SIFIVE_COMPATIBLES: [&str; 1] = ["sifive,uart0"];
const UART_PL011_COMPATIBLES: [&str; 2] = ["pl011", "arm,pl011"];
const UART_XSCALE_COMPATIBLES: [&str; 1] = ["intel,xscale-uart"];
const UART_SPACEMIT_K1_COMPATIBLES: [&str; 1] = ["spacemit,k1-uart"];

impl ConsoleKind {
    /// Returns whether `compatible` names a console family supported by this
    /// firmware, independently of its register layout.
    pub(crate) fn supports(compatible: &str) -> bool {
        compatible == "ns16550"
            || UART_16550A_COMPATIBLES.contains(&compatible)
            || UART_16550_U32_COMPATIBLES.contains(&compatible)
            || UART_AXI_LITE_COMPATIBLES.contains(&compatible)
            || UART_BFLB_COMPATIBLES.contains(&compatible)
            || UART_SIFIVE_COMPATIBLES.contains(&compatible)
            || UART_PL011_COMPATIBLES.contains(&compatible)
            || UART_XSCALE_COMPATIBLES.contains(&compatible)
            || UART_SPACEMIT_K1_COMPATIBLES.contains(&compatible)
    }

    /// Maps one `compatible` string plus the node's `reg-shift` and
    /// `reg-io-width` properties to a driver, or `None` when unsupported.
    pub(crate) fn from_fdt(
        compatible: &str,
        register_shift: Option<u32>,
        register_width: Option<u32>,
    ) -> Option<Self> {
        let u8_layout = register_shift.unwrap_or(0) == 0 && register_width.unwrap_or(1) == 1;
        let u32_layout = register_shift == Some(2) && register_width == Some(4);

        if UART_16550A_COMPATIBLES.contains(&compatible) {
            if u8_layout {
                Some(Self::Uart16550U8)
            } else if u32_layout {
                Some(Self::Uart16550U32)
            } else {
                None
            }
        } else if compatible == "ns16550" {
            if u32_layout {
                // SpacemiT K1 firmware describes its XScale-compatible UART
                // as a word-wide `ns16550` and requires the UUE enable bit.
                Some(Self::SpacemitK1)
            } else if u8_layout {
                Some(Self::Uart16550U8)
            } else {
                None
            }
        } else if UART_16550_U32_COMPATIBLES.contains(&compatible) {
            // Preserve the pre-MMIO driver's identity-based selection even
            // when older device trees omit the layout properties.
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
        } else if UART_SPACEMIT_K1_COMPATIBLES.contains(&compatible) {
            Some(Self::SpacemitK1)
        } else {
            None
        }
    }

    /// Device name reported in boot logs.
    pub(crate) fn name(self) -> &'static str {
        match self {
            ConsoleKind::Uart16550U8 => "Uart16550U8",
            ConsoleKind::Uart16550U32 => "Uart16550U32",
            ConsoleKind::AxiLite => "UartAxiLite",
            ConsoleKind::Bl808 => "UartBl808",
            ConsoleKind::SiFive => "UartSiFive",
            ConsoleKind::Pl011 => "UartPl011",
            ConsoleKind::XScale => "UartXScale",
            ConsoleKind::SpacemitK1 => "UartSpacemitK1",
        }
    }
}
