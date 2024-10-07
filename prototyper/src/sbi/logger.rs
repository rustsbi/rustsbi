use core::str::FromStr;
use log::{Level, LevelFilter};

pub struct Logger;

impl Logger {
    pub fn init() {
        log::set_max_level(
            option_env!("RUST_LOG")
                .and_then(|s| LevelFilter::from_str(s).ok())
                .unwrap_or(LevelFilter::Info),
        );
        log::set_logger(&Logger).unwrap();
    }
}

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
