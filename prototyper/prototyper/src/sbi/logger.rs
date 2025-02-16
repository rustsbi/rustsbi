use core::str::FromStr;
use log::{Level, LevelFilter};

/// Simple logger implementation for RustSBI that supports colored output.
pub struct Logger;

impl Logger {
    /// Initialize the logger with log level from RUST_LOG env var or default to Info.
    pub fn init() -> Result<(), log::SetLoggerError> {
        // Set max log level from LOG_LEVEL from config file, otherwise use Info
        let max_level = LevelFilter::from_str(crate::cfg::LOG_LEVEL).unwrap_or(LevelFilter::Info);

        log::set_max_level(max_level);
        log::set_logger(&Logger)
    }
}

impl log::Log for Logger {
    // Always enable logging for all log levels
    #[inline]
    fn enabled(&self, _metadata: &log::Metadata) -> bool {
        true
    }

    // Log messages with color-coded levels
    #[inline]
    fn log(&self, record: &log::Record) {
        // ANSI color codes for different log levels
        const ERROR_COLOR: u8 = 31; // Red
        const WARN_COLOR: u8 = 93; // Bright yellow
        const INFO_COLOR: u8 = 32; // Green
        const DEBUG_COLOR: u8 = 36; // Cyan
        const TRACE_COLOR: u8 = 90; // Bright black

        let color_code = match record.level() {
            Level::Error => ERROR_COLOR,
            Level::Warn => WARN_COLOR,
            Level::Info => INFO_COLOR,
            Level::Debug => DEBUG_COLOR,
            Level::Trace => TRACE_COLOR,
        };

        println!(
            "\x1b[1;37m[RustSBI] \x1b[1;{color_code}m{:^5}\x1b[0m - {}",
            record.level(),
            record.args(),
        );
    }

    // No-op flush since we use println! which is already line-buffered
    #[inline]
    fn flush(&self) {}
}
