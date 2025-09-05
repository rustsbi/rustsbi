use arm_pl011_uart::{
    DataBits, LineConfig, PL011Registers, Parity, StopBits, Uart, UniqueMmioPointer,
};
use bouffalo_hal::uart::RegisterBlock as BflbUartRegisterBlock;
use core::cell::UnsafeCell;
use core::ptr::NonNull;
use uart_sifive::MmioUartSifive;
use uart_xilinx::MmioUartAxiLite;
use uart16550::{Register, Uart16550};

use crate::sbi::console::ConsoleDevice;

pub(crate) const UART16650U8_COMPATIBLE: [&str; 1] = ["ns16550a"];
pub(crate) const UART16650U32_COMPATIBLE: [&str; 1] = ["snps,dw-apb-uart"];
pub(crate) const UARTAXILITE_COMPATIBLE: [&str; 1] = ["xlnx,xps-uartlite-1.00.a"];
pub(crate) const UARTBFLB_COMPATIBLE: [&str; 1] = ["bflb,bl808-uart"];
pub(crate) const UARTSIFIVE_COMPATIBLE: [&str; 1] = ["sifive,uart0"];
pub(crate) const UARTPL011_COMPATIBLE: [&str; 1] = ["pl011"];

#[doc(hidden)]
#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub enum MachineConsoleType {
    Uart16550U8,
    Uart16550U32,
    UartAxiLite,
    UartBflb,
    UartSifive,
    UartPl011,
}

/// For Uart 16550
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

/// For Uart AxiLite
impl ConsoleDevice for MmioUartAxiLite {
    fn read(&self, buf: &mut [u8]) -> usize {
        self.read(buf)
    }

    fn write(&self, buf: &[u8]) -> usize {
        self.write(buf)
    }
}

/// Wrapper of UartSifive, warp for initialization.
pub struct UartSifiveWrap {
    inner: MmioUartSifive,
}

impl UartSifiveWrap {
    pub fn new(addr: usize) -> Self {
        let inner = MmioUartSifive::new(addr);
        inner.disable_interrupt();
        inner.enable_read();
        inner.enable_write();
        // TODO: calcuate & set div register
        Self { inner }
    }
}

/// For Uart Sifive
impl ConsoleDevice for UartSifiveWrap {
    fn read(&self, buf: &mut [u8]) -> usize {
        self.inner.read(buf)
    }

    fn write(&self, buf: &[u8]) -> usize {
        self.inner.write(buf)
    }
}

/// For Uart BFLB
pub struct UartBflbWrap {
    inner: *const BflbUartRegisterBlock,
}

impl UartBflbWrap {
    pub fn new(base: usize) -> Self {
        Self {
            inner: base as *const BflbUartRegisterBlock,
        }
    }
}

impl ConsoleDevice for UartBflbWrap {
    fn read(&self, buf: &mut [u8]) -> usize {
        let uart = unsafe { &(*self.inner) };
        while uart.fifo_config_1.read().receive_available_bytes() == 0 {
            core::hint::spin_loop();
        }
        let len = core::cmp::min(
            uart.fifo_config_1.read().receive_available_bytes() as usize,
            buf.len(),
        );
        buf.iter_mut()
            .take(len)
            .for_each(|slot| *slot = uart.fifo_read.read());
        len
    }

    fn write(&self, buf: &[u8]) -> usize {
        let uart = unsafe { &(*self.inner) };
        let mut count = 0;
        for current in buf {
            if uart.fifo_config_1.read().transmit_available_bytes() == 0 {
                break;
            }
            count += 1;
            unsafe {
                uart.fifo_write.write(*current);
            }
        }
        count
    }
}

/// PL011 UART wrapper for RustSBI console
pub struct UartPl011Wrap {
    uart: UnsafeCell<Uart<'static>>,
}

impl UartPl011Wrap {
    /// Create a new PL011 UART wrapper
    pub fn new(base: usize) -> Self {
        let uart_pointer =
            unsafe { UniqueMmioPointer::new(NonNull::new(base as *mut PL011Registers).unwrap()) };

        let mut uart = Uart::new(uart_pointer);

        // Configure and enable UART with default settings
        let line_config = LineConfig {
            data_bits: DataBits::Bits8,
            parity: Parity::None,
            stop_bits: StopBits::One,
        };
        if let Err(_) = uart.enable(line_config, 115_200, 24_000_000) {
            // If enabling fails, we still create the wrapper but it may not work properly
        }
        Self {
            uart: UnsafeCell::new(uart),
        }
    }

    unsafe fn uart_mut(&self) -> &mut Uart<'static> {
        unsafe { &mut *self.uart.get() }
    }
}

unsafe impl Send for UartPl011Wrap {}
unsafe impl Sync for UartPl011Wrap {}

impl ConsoleDevice for UartPl011Wrap {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;

        let uart = unsafe { self.uart_mut() };

        for slot in buf.iter_mut() {
            match uart.read_word() {
                Ok(Some(byte)) => {
                    *slot = byte;
                    count += 1;
                }
                Ok(None) => break,
                Err(_) => break,
            }
        }

        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        let uart = unsafe { self.uart_mut() };

        for &byte in buf {
            uart.write_word(byte);
        }

        buf.len()
    }
}
