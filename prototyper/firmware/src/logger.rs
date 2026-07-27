//! Serialized upper-policy logging through the machine Console capability.

use alloc::vec;
use core::panic::Location;

use log::{LevelFilter, Log, Metadata, Record};
use spin::Once;

static LOGGER: Once<Logger> = Once::new();

struct Logger {
    console: machine::Console,
    hart_state: machine::HartLocal<LoggerState>,
}

#[derive(Clone, Copy)]
struct LoggerState;

pub(super) fn install(console: machine::Console, hart_count: usize) -> bool {
    let Ok(hart_state) = machine::HartLocal::new(vec![LoggerState; hart_count]) else {
        return false;
    };
    let logger = LOGGER.call_once(|| Logger {
        console,
        hart_state,
    });
    if log::set_logger(logger).is_err() {
        return false;
    }
    log::set_max_level(LevelFilter::Info);
    true
}

pub(super) fn report_panic(location: Option<&Location<'_>>) {
    let Some(logger) = LOGGER.get() else {
        return;
    };
    match location {
        Some(location) => {
            let _ = logger.console.try_write_fmt(format_args!(
                "[RustSBI] panic at {}:{}:{}\n",
                location.file(),
                location.line(),
                location.column(),
            ));
        }
        None => {
            let _ = logger
                .console
                .try_write_fmt(format_args!("[RustSBI] panic\n"));
        }
    }
}

impl Log for Logger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= LevelFilter::Info
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        // The guard covers formatting as well as output. It rejects recursive
        // logging on this hart and masks local interrupts until the complete
        // record has left the shared Console capability.
        let Ok(_hart) = self.hart_state.current() else {
            return;
        };
        let _ = self.console.write_fmt(format_args!(
            "[RustSBI] {:5} - {}\n",
            record.level(),
            record.args(),
        ));
    }

    fn flush(&self) {}
}
