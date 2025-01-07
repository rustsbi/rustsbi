use uart16550::{Register, Uart16550};
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

pub struct Uart16550Wrap<R: Register> {
    inner: *const Uart16550<R>,
}

impl<R: Register> Uart16550Wrap<R> {
    pub fn new(base: usize) -> Self {
        Self {
            inner: base as *const Uart16550<R>,
        }
    }
}

impl<R: Register> ConsoleDevice for Uart16550Wrap<R> {
    fn read(&self, buf: &mut [u8]) -> usize {
        unsafe { (*self.inner).read(buf) }
    }

    fn write(&self, buf: &[u8]) -> usize {
        unsafe { (*self.inner).write(buf) }
    }
}

impl ConsoleDevice for MmioUartAxiLite {
    fn read(&self, buf: &mut [u8]) -> usize {
        self.read(buf)
    }

    fn write(&self, buf: &[u8]) -> usize {
        self.write(buf)
    }
}
