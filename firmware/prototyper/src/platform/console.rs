use arm_pl011_uart::{
    DataBits, LineConfig, PL011Registers, Parity, StopBits, Uart, UniqueMmioPointer,
};
use bouffalo_hal::uart::RegisterBlock as BflbUartRegisterBlock;
use core::cell::UnsafeCell;
use core::ptr::NonNull;
use uart_sifive::MmioUartSifive;
use uart_xilinx::MmioUartAxiLite;
use uart_xscale::UartXscale;
use uart16550::{Register, Uart16550};

use crate::sbi::console::ConsoleDevice;

pub(crate) const UART16650U8_COMPATIBLE: [&str; 1] = ["ns16550a"];
pub(crate) const UART16650U32_COMPATIBLE: [&str; 1] = ["snps,dw-apb-uart"];
pub(crate) const UARTAXILITE_COMPATIBLE: [&str; 1] = ["xlnx,xps-uartlite-1.00.a"];
pub(crate) const UARTBFLB_COMPATIBLE: [&str; 1] = ["bflb,bl808-uart"];
pub(crate) const UARTSIFIVE_COMPATIBLE: [&str; 1] = ["sifive,uart0"];
pub(crate) const UARTPL011_COMPATIBLE: [&str; 1] = ["pl011"];
pub(crate) const UARTXSCALE_COMPATIBLE: [&str; 4] = [
    "intel,xscale-uart",
    "spacemit,k1-uart",
    "spacemit,k3-uart",
    // Official OrangePi RV2 U-Boot (orangepi-xunlong/u-boot-orangepi,
    // v2022.10-ky) describes uart0 as plain "ns16550" with reg-io-width=4 and
    // drives it as the XScale variant (CONFIG_SYS_NS16550_IER=0x40 = UUE).
    "ns16550",
];

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
    UartXscale,
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

// SAFETY: `Uart16550Wrap` holds only a raw pointer to MMIO registers and no
// Rust-side mutable state; moving the handle across harts is sound. Access
// is serialized by the mutex around the published console device.
unsafe impl<R: Register> Send for Uart16550Wrap<R> {}

impl<R: Register> ConsoleDevice for Uart16550Wrap<R> {
    fn read(&self, buf: &mut [u8]) -> usize {
        unsafe { (*self.inner).read(buf) }
    }

    fn write(&self, buf: &[u8]) -> usize {
        unsafe { (*self.inner).write(buf) }
    }
}

/// For Uart AxiLite
pub struct UartAxiLiteWrap {
    inner: MmioUartAxiLite,
}

impl UartAxiLiteWrap {
    pub fn new(base: usize) -> Self {
        Self {
            inner: MmioUartAxiLite::new(base),
        }
    }
}

// SAFETY: `MmioUartAxiLite` is a volatile MMIO handle with no Rust-side
// mutable state; access is serialized by the mutex around the published
// console device. The newtype exists so this assertion can be made locally
// (orphan rule: both `Send` and the UART type are foreign).
unsafe impl Send for UartAxiLiteWrap {}

impl ConsoleDevice for UartAxiLiteWrap {
    fn read(&self, buf: &mut [u8]) -> usize {
        self.inner.read(buf)
    }

    fn write(&self, buf: &[u8]) -> usize {
        self.inner.write(buf)
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

// SAFETY: `UartSifiveWrap` only forwards to a volatile MMIO handle with no
// Rust-side mutable state; access is serialized by the mutex around the
// published console device.
unsafe impl Send for UartSifiveWrap {}

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

// SAFETY: `UartBflbWrap` only forwards to a volatile MMIO handle with no
// Rust-side mutable state; access is serialized by the mutex around the
// published console device.
unsafe impl Send for UartBflbWrap {}

impl ConsoleDevice for UartBflbWrap {
    fn read(&self, buf: &mut [u8]) -> usize {
        let uart = unsafe { &(*self.inner) };
        let rx_available = uart.fifo_config_1.read().receive_available_bytes() as usize;
        if rx_available == 0 {
            return 0;
        }
        let len = core::cmp::min(rx_available, buf.len());
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
}

unsafe impl Send for UartPl011Wrap {}
unsafe impl Sync for UartPl011Wrap {}

impl ConsoleDevice for UartPl011Wrap {
    fn read(&self, buf: &mut [u8]) -> usize {
        let mut count = 0;

        let uart = unsafe { &mut *self.uart.get() };

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
        let uart = unsafe { &mut *self.uart.get() };

        for &byte in buf {
            uart.write_word(byte);
        }

        buf.len()
    }
}

/// Intel XScale/PXA UART wrapper for SpacemiT K1 / Ky X1 SoC.
///
/// Uses the `uart-xscale` crate which handles the UUE (UART Unit Enable)
/// bit required by Intel XScale/PXA UARTs, with 32-bit MMIO access and
/// stride=4 (reg-shift=2).
pub struct UartXscaleWrap {
    inner: UnsafeCell<UartXscale>,
}

impl UartXscaleWrap {
    /// Create a new XScale UART wrapper at the given MMIO base address.
    ///
    pub fn new(base: usize, clock_freq: Option<u32>) -> Self {
        let mut inner = UartXscale::new(base);
        inner.init(clock_freq.unwrap_or(14_857_000), 115_200);
        Self {
            inner: UnsafeCell::new(inner),
        }
    }
}

unsafe impl Send for UartXscaleWrap {}
unsafe impl Sync for UartXscaleWrap {}

impl ConsoleDevice for UartXscaleWrap {
    fn read(&self, buf: &mut [u8]) -> usize {
        // SAFETY: `UartXscale` performs volatile MMIO access; the outer mutex in
        // the published console device serializes callers, so the borrowed
        // handle cannot race.
        // SbiConsole serializes access in a single-threaded SBI context.
        let uart = unsafe { &mut *self.inner.get() };
        let mut count = 0;
        for slot in buf.iter_mut() {
            if let Some(c) = uart.try_getchar() {
                *slot = c;
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    fn write(&self, buf: &[u8]) -> usize {
        // SAFETY: same justification as `read`: volatile MMIO access under the
        // published console device's mutex.
        let uart = unsafe { &mut *self.inner.get() };
        for &c in buf {
            uart.putchar(c);
        }
        buf.len()
    }
}
