//! Console backend selection from FDT identity and register layout.

/// The console device kinds the firmware can drive.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ConsoleKind {
    Uart16550Byte,
    Uart16550Word,
    AxiLite,
    Bflb,
    Sifive,
    Pl011,
    Xscale,
}

const UART_16550A_COMPATIBLES: [&str; 1] = ["ns16550a"];
const UART_16550_WORD_COMPATIBLES: [&str; 1] = ["snps,dw-apb-uart"];
const UART_AXI_LITE_COMPATIBLES: [&str; 1] = ["xlnx,xps-uartlite-1.00.a"];
const UART_BFLB_COMPATIBLES: [&str; 1] = ["bflb,bl808-uart"];
const UART_SIFIVE_COMPATIBLES: [&str; 1] = ["sifive,uart0"];
const UART_PL011_COMPATIBLES: [&str; 2] = ["pl011", "arm,pl011"];
const UART_XSCALE_COMPATIBLES: [&str; 2] = ["intel,xscale-uart", "spacemit,k1-uart"];

impl ConsoleKind {
    /// Maps one `compatible` string plus the node's `reg-shift` and
    /// `reg-io-width` properties to a driver, or `None` when unsupported.
    pub(crate) fn from_fdt(
        compatible: &str,
        register_shift: Option<u32>,
        register_width: Option<u32>,
    ) -> Option<Self> {
        let byte_layout = register_shift.unwrap_or(0) == 0 && register_width.unwrap_or(1) == 1;
        let word_layout = register_shift == Some(2) && register_width == Some(4);

        if UART_16550A_COMPATIBLES.contains(&compatible) {
            if byte_layout {
                Some(Self::Uart16550Byte)
            } else if word_layout {
                Some(Self::Uart16550Word)
            } else {
                None
            }
        } else if compatible == "ns16550" {
            if word_layout {
                // SpacemiT K1 firmware describes its XScale-compatible UART
                // as a word-wide `ns16550` and requires the UUE enable bit.
                Some(Self::Xscale)
            } else if byte_layout {
                Some(Self::Uart16550Byte)
            } else {
                None
            }
        } else if UART_16550_WORD_COMPATIBLES.contains(&compatible) {
            // Preserve the pre-MMIO driver's identity-based selection even
            // when older device trees omit the layout properties.
            Some(Self::Uart16550Word)
        } else if UART_AXI_LITE_COMPATIBLES.contains(&compatible) {
            Some(Self::AxiLite)
        } else if UART_BFLB_COMPATIBLES.contains(&compatible) {
            Some(Self::Bflb)
        } else if UART_SIFIVE_COMPATIBLES.contains(&compatible) {
            Some(Self::Sifive)
        } else if UART_PL011_COMPATIBLES.contains(&compatible) {
            Some(Self::Pl011)
        } else if UART_XSCALE_COMPATIBLES.contains(&compatible) {
            Some(Self::Xscale)
        } else {
            None
        }
    }

    /// Device name reported in boot logs.
    pub(crate) fn name(self) -> &'static str {
        match self {
            ConsoleKind::Uart16550Byte => "Uart16550U8",
            ConsoleKind::Uart16550Word => "Uart16550U32",
            ConsoleKind::AxiLite => "UartAxiLite",
            ConsoleKind::Bflb => "UartBflb",
            ConsoleKind::Sifive => "UartSifive",
            ConsoleKind::Pl011 => "UartPl011",
            ConsoleKind::Xscale => "UartXscale",
        }
    }
}
