use std::ffi::OsString;
use std::process::{Command, ExitStatus};

/// Temporary compatibility bridge to the sole Prototyper tooling
/// implementation. Remove this module with the legacy xtask variants.
pub(crate) fn run(arguments: impl IntoIterator<Item = OsString>) -> Option<ExitStatus> {
    eprintln!("warning: `cargo xtask prototyper|test|bench` is deprecated; use `cargo prototyper`");
    Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["run", "--package", "cargo-prototyper", "--"])
        .args(arguments)
        .status()
        .ok()
}
