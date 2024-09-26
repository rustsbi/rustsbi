use core::{fmt, ptr::null, str::FromStr};
use log::{Level, LevelFilter};
use rustsbi::{Console, Physical, SbiRet};
use spin::Mutex;
use uart16550::Uart16550;

pub struct ConsoleDevice<'a> {
    inner: &'a Mutex<MachineConsole>,
}

impl<'a> ConsoleDevice<'a> {
    pub fn new(inner: &'a Mutex<MachineConsole>) -> Self {
        Self { inner }
    }
}

#[doc(hidden)]
pub(crate) static CONSOLE: Mutex<MachineConsole> = Mutex::new(MachineConsole::Uart16550(null()));

pub fn init(base: usize) {
    *CONSOLE.lock() = MachineConsole::Uart16550(base as _);
    log_init();
}

impl<'a> Console for ConsoleDevice<'a> {
    #[inline]
    fn write(&self, bytes: Physical<&[u8]>) -> SbiRet {
        // TODO verify valid memory range for a `Physical` slice.
        let start = bytes.phys_addr_lo();
        let buf = unsafe { core::slice::from_raw_parts(start as *const u8, bytes.num_bytes()) };
        let console = self.inner.lock();
        let bytes_num: usize = match *console {
            MachineConsole::Uart16550(uart16550) => unsafe { (*uart16550).write(buf) },
        };
        drop(console);
        SbiRet::success(bytes_num)
    }

    #[inline]
    fn read(&self, bytes: Physical<&mut [u8]>) -> SbiRet {
        // TODO verify valid memory range for a `Physical` slice.
        let start = bytes.phys_addr_lo();
        let buf = unsafe { core::slice::from_raw_parts_mut(start as *mut u8, bytes.num_bytes()) };
        let console = self.inner.lock();
        let bytes_num: usize = match *console {
            MachineConsole::Uart16550(uart16550) => unsafe { (*uart16550).read(buf) },
        };
        drop(console);
        SbiRet::success(bytes_num)
    }

    #[inline]
    fn write_byte(&self, byte: u8) -> SbiRet {
        let console = self.inner.lock();
        let bytes_num: usize = match *console {
            MachineConsole::Uart16550(uart16550) => unsafe { (*uart16550).write(&[byte]) },
        };
        drop(console);
        SbiRet::success(bytes_num)
    }
}

#[doc(hidden)]
pub enum MachineConsole {
    Uart16550(*const Uart16550<u8>),
}

impl fmt::Write for MachineConsole {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let mut bytes = s.as_bytes();
        match self {
            Self::Uart16550(uart16550) => {
                while !bytes.is_empty() {
                    let count = unsafe { &**uart16550 }.write(bytes);
                    bytes = &bytes[count..];
                }
            }
        }
        Ok(())
    }
}

unsafe impl Send for MachineConsole {}
unsafe impl Sync for MachineConsole {}

pub fn log_init() {
    log::set_max_level(
        option_env!("RUST_LOG")
            .and_then(|s| LevelFilter::from_str(s).ok())
            .unwrap_or(LevelFilter::Info),
    );
    log::set_logger(&Logger).unwrap();
}

struct Logger;

impl log::Log for Logger {
    #[inline]
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    #[inline]
    fn log(&self, record: &log::Record) {
        let color_code: u8 = match record.level() {
            Level::Error => 31,
            Level::Warn => 93,
            Level::Info => 34,
            Level::Debug => 32,
            Level::Trace => 90,
        };
        println!(
            "\x1b[{color_code}m[{:>5}] {}\x1b[0m",
            record.level(),
            record.args(),
        );
    }

    fn flush(&self) {}
}

// pub fn load_console_uart16550(uart16550: &Uart16550<u8>) {
//     let mut console = CONSOLE.lock();
//     *console = MachineConsole::Uart16550(uart16550);
//     drop(console);
// }
