//! Error context for boot-hart platform initialization.

use core::fmt;
use runtime::{Error, Result};

#[derive(Debug)]
pub(super) struct InitError {
    operation: &'static str,
    source: Error,
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

pub(super) trait ResultContext<T> {
    fn during(self, operation: &'static str) -> Result<T, InitError>;
}

impl<T> ResultContext<T> for Result<T> {
    fn during(self, operation: &'static str) -> Result<T, InitError> {
        self.map_err(|source| InitError { operation, source })
    }
}
