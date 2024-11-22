use aclint::SifiveClint;
use core::{
    ops::Range,
    ptr::null_mut,
    sync::atomic::{AtomicPtr, Ordering::Release},
};
use sifive_test_device::SifiveTestDevice;
use spin::Mutex;
use uart16550::Uart16550;
use uart_xilinx::uart_lite::uart::MmioUartAxiLite;

use crate::sbi::console::ConsoleDevice;
use crate::sbi::ipi::IpiDevice;
use crate::sbi::reset::ResetDevice;
use crate::sbi::Sbi;

pub struct Device {
    pub memory_range: Option<Range<usize>>,
    pub uart: Option<Mutex<MachineConsole>>,
    pub sifive_test: AtomicPtr<SifiveTestDevice>,
    pub sifive_clint: AtomicPtr<SifiveClint>,
}

pub struct Board<'a> {
    pub sbi: Sbi<'a, MachineConsole, SifiveClint, SifiveTestDevice>,
    pub device: Device,
}

pub(crate) static mut BOARD: Board<'static> = Board {
    device: Device {
        memory_range: None,
        uart: None,
        sifive_test: AtomicPtr::new(null_mut()),
        sifive_clint: AtomicPtr::new(null_mut()),
    },
    sbi: Sbi {
        console: None,
        ipi: None,
        reset: None,
        hsm: None,
        rfence: None,
    },
};

/// Console Device: Uart16550
#[doc(hidden)]
#[allow(unused)]
pub enum MachineConsoleType {
    Uart16550,
    UartAxiLite,
}
#[doc(hidden)]
#[allow(unused)]
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

#[doc(hidden)]
pub(crate) fn console_dev_init(console_type: MachineConsoleType, base: usize) {
    let new_console = match console_type {
        MachineConsoleType::Uart16550 => MachineConsole::Uart16550(base as _),
        MachineConsoleType::UartAxiLite => MachineConsole::UartAxiLite(MmioUartAxiLite::new(base)),
    };
    unsafe { BOARD.device.uart = Some(Mutex::new(new_console)) };
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
pub(crate) fn ipi_dev_init(base: usize) {
    unsafe {
        BOARD.device.sifive_clint.store(base as _, Release);
    }
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
pub fn reset_dev_init(base: usize) {
    unsafe {
        BOARD.device.sifive_test.store(base as _, Release);
    }
}
