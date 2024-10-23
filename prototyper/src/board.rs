use aclint::SifiveClint;
use core::mem::MaybeUninit;
use core::{
    ptr::{null, null_mut},
    sync::atomic::{AtomicPtr, Ordering::Release},
};
use sifive_test_device::SifiveTestDevice;
use spin::Mutex;
use uart16550::Uart16550;
use uart_xilinx::uart_lite::uart::MmioUartAxiLite;

use crate::sbi::console::ConsoleDevice;
use crate::sbi::ipi::IpiDevice;
use crate::sbi::reset::ResetDevice;
use crate::sbi::SBI;

pub(crate) static mut SBI_IMPL: MaybeUninit<
    SBI<'static, MachineConsole, SifiveClint, SifiveTestDevice>,
> = MaybeUninit::uninit();

/// Console Device: Uart16550
#[doc(hidden)]
pub enum MachineConsole {
    Uart16550(*const Uart16550<u8>),
    UartAxiLite(MmioUartAxiLite),
}

unsafe impl Send for MachineConsole {}
unsafe impl Sync for MachineConsole {}

impl ConsoleDevice for MachineConsole {
    fn read(&self, buf: &mut [u8]) -> usize {
        match self {
            Self::Uart16550(uart16550) => unsafe { (**uart16550).read(buf) },
            Self::UartAxiLite(axilite) => axilite.read(buf),
        }
    }

    fn write(&self, buf: &[u8]) -> usize {
        match self {
            MachineConsole::Uart16550(uart16550) => unsafe { (**uart16550).write(buf) },
            Self::UartAxiLite(axilite) => axilite.write(buf),
        }
    }
}

// TODO: select driver follow fdt

#[doc(hidden)]
#[cfg(feature = "nemu")]
pub(crate) static UART: Mutex<MachineConsole> =
    Mutex::new(MachineConsole::UartAxiLite(MmioUartAxiLite::new(0)));
#[cfg(not(feature = "nemu"))]
pub(crate) static UART: Mutex<MachineConsole> = Mutex::new(MachineConsole::Uart16550(null()));
pub(crate) fn console_dev_init(base: usize) {
    let new_console = match *UART.lock() {
        MachineConsole::Uart16550(_) => MachineConsole::Uart16550(base as _),
        MachineConsole::UartAxiLite(_) => MachineConsole::UartAxiLite(MmioUartAxiLite::new(base)),
    };
    *UART.lock() = new_console;
}

/// Ipi Device: Sifive Clint
impl IpiDevice for SifiveClint {
    #[inline(always)]
    fn read_mtime(&self) -> u64 {
        self.read_mtime()
    }

    #[inline(always)]
    fn write_mtime(&self, val: u64) {
        self.write_mtime(val)
    }

    #[inline(always)]
    fn read_mtimecmp(&self, hart_idx: usize) -> u64 {
        self.read_mtimecmp(hart_idx)
    }

    #[inline(always)]
    fn write_mtimecmp(&self, hart_idx: usize, val: u64) {
        self.write_mtimecmp(hart_idx, val)
    }

    #[inline(always)]
    fn read_msip(&self, hart_idx: usize) -> bool {
        self.read_msip(hart_idx)
    }

    #[inline(always)]
    fn set_msip(&self, hart_idx: usize) {
        self.set_msip(hart_idx)
    }

    #[inline(always)]
    fn clear_msip(&self, hart_idx: usize) {
        self.clear_msip(hart_idx)
    }
}

#[doc(hidden)]
pub(crate) static SIFIVECLINT: AtomicPtr<SifiveClint> = AtomicPtr::new(null_mut());
pub(crate) fn ipi_dev_init(base: usize) {
    SIFIVECLINT.store(base as _, Release);
}

/// Reset Device: SifiveTestDevice
impl ResetDevice for SifiveTestDevice {
    #[inline]
    fn fail(&self, code: u16) -> ! {
        self.fail(code)
    }

    #[inline]
    fn pass(&self) -> ! {
        self.pass()
    }

    #[inline]
    fn reset(&self) -> ! {
        self.reset()
    }
}

#[doc(hidden)]
pub(crate) static SIFIVETEST: AtomicPtr<SifiveTestDevice> = AtomicPtr::new(null_mut());
pub fn reset_dev_init(base: usize) {
    SIFIVETEST.store(base as _, Release);
}
