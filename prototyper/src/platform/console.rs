use uart16550::Uart16550;
use uart_xilinx::MmioUartAxiLite;

use crate::sbi::console::ConsoleDevice;
pub(crate) const UART16650U8_COMPATIBLE: [&str; 1] = ["ns16550a"];
pub(crate) const UART16650U32_COMPATIBLE: [&str; 1] = ["snps,dw-apb-uart"];
pub(crate) const UARTAXILITE_COMPATIBLE: [&str; 1] = ["xlnx,xps-uartlite-1.00.a"];

#[doc(hidden)]
#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub enum MachineConsoleType {
    Uart16550U8,
    Uart16550U32,
    UartAxiLite,
}
#[doc(hidden)]
#[allow(unused)]
pub enum MachineConsole {
    Uart16550U8(*const Uart16550<u8>),
    Uart16550U32(*const Uart16550<u32>),
    UartAxiLite(MmioUartAxiLite),
}

unsafe impl Send for MachineConsole {}
unsafe impl Sync for MachineConsole {}

impl ConsoleDevice for MachineConsole {
    fn read(&self, buf: &mut [u8]) -> usize {
        match self {
            Self::Uart16550U8(uart16550) => unsafe { (**uart16550).read(buf) },
            Self::Uart16550U32(uart16550) => unsafe { (**uart16550).read(buf) },
            Self::UartAxiLite(axilite) => axilite.read(buf),
        }
    }

    fn write(&self, buf: &[u8]) -> usize {
        match self {
            MachineConsole::Uart16550U8(uart16550) => unsafe { (**uart16550).write(buf) },
            MachineConsole::Uart16550U32(uart16550) => unsafe { (**uart16550).write(buf) },
            Self::UartAxiLite(axilite) => axilite.write(buf),
        }
    }
}
