//! Error context for boot-hart platform initialization.

use core::fmt;

#[derive(Debug)]
pub(super) struct InitError {
    operation: &'static str,
    source: runtime::Error,
}

impl fmt::Display for InitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "platform initialization failed while {}: {}",
            self.operation, self.source
        )
    }
}

pub(super) type Result<T> = core::result::Result<T, InitError>;

pub(super) trait ResultContext<T> {
    fn during(self, operation: &'static str) -> Result<T>;
}

impl<T> ResultContext<T> for runtime::Result<T> {
    fn during(self, operation: &'static str) -> Result<T> {
        self.map_err(|source| InitError { operation, source })
    }
}
