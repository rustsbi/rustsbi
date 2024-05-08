use core::{
    fmt::{self, Write},
    str::FromStr,
};
use log::{Level, LevelFilter};
use spin::Mutex;
use uart16550::Uart16550;

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

#[doc(hidden)]
pub static CONSOLE: Mutex<MachineConsole> =
    Mutex::new(MachineConsole::Uart16550(0x10000000 as *const _));

pub fn init() {
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
