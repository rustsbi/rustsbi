//! Convert ELF firmware/kernel images to raw binaries.
//!
//! Prefer `rust-objcopy` (from cargo-binutils). When that wrapper is not on
//! `PATH`, fall back to `llvm-objcopy` with the same `-O binary` arguments.
//! If the chosen tool starts but exits unsuccessfully, do not try the other.

use std::{
    ffi::OsStr,
    io,
    path::Path,
    process::{Command, ExitStatus},
};

use anyhow::{Context, Result};
use log::info;

const RUST_OBJCOPY: &str = "rust-objcopy";
const LLVM_OBJCOPY: &str = "llvm-objcopy";

/// Which objcopy tool to run given availability of each on `PATH`.
///
/// Used by unit tests so priority stays locked without spawning tools. The
/// runtime path mirrors this order via `NotFound` from `Command::new`.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn choose_objcopy(rust_available: bool, llvm_available: bool) -> Option<&'static str> {
    if rust_available {
        Some(RUST_OBJCOPY)
    } else if llvm_available {
        Some(LLVM_OBJCOPY)
    } else {
        None
    }
}

fn is_not_found(err: &io::Error) -> bool {
    err.kind() == io::ErrorKind::NotFound
}

fn run_tool(tool: &str, args: &[&OsStr]) -> io::Result<ExitStatus> {
    Command::new(tool).args(args).status()
}

/// Run `-O binary <elf> <bin>` with `rust-objcopy`, or `llvm-objcopy` if the
/// wrapper is missing. Returns the tool's exit status when it starts; does not
/// fall back after a non-zero exit.
pub(crate) fn convert_elf_to_binary(elf: &Path, bin: &Path) -> Result<ExitStatus> {
    let elf_s = elf.as_os_str();
    let bin_s = bin.as_os_str();
    let args: [&OsStr; 4] = [OsStr::new("-O"), OsStr::new("binary"), elf_s, bin_s];

    match run_tool(RUST_OBJCOPY, &args) {
        Ok(status) => Ok(status),
        Err(err) if is_not_found(&err) => {
            info!("{RUST_OBJCOPY} not found; trying {LLVM_OBJCOPY}");
            match run_tool(LLVM_OBJCOPY, &args) {
                Ok(status) => Ok(status),
                Err(llvm_err) if is_not_found(&llvm_err) => Err(llvm_err).with_context(|| {
                    format!(
                        "failed to execute {RUST_OBJCOPY} or {LLVM_OBJCOPY}: '{}' -> '{}'; \
                         install cargo-binutils (`cargo install --locked cargo-binutils@0.4.0`) \
                         or ensure llvm-objcopy is on PATH",
                        elf.display(),
                        bin.display()
                    )
                }),
                Err(llvm_err) => Err(llvm_err).with_context(|| {
                    format!(
                        "failed to execute {LLVM_OBJCOPY}: '{}' -> '{}'",
                        elf.display(),
                        bin.display()
                    )
                }),
            }
        }
        Err(err) => Err(err).with_context(|| {
            format!(
                "failed to execute {RUST_OBJCOPY}: '{}' -> '{}'",
                elf.display(),
                bin.display()
            )
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_rust_objcopy_when_available() {
        assert_eq!(choose_objcopy(true, true), Some(RUST_OBJCOPY));
        assert_eq!(choose_objcopy(true, false), Some(RUST_OBJCOPY));
    }

    #[test]
    fn falls_back_to_llvm_objcopy_when_rust_missing() {
        assert_eq!(choose_objcopy(false, true), Some(LLVM_OBJCOPY));
    }

    #[test]
    fn reports_none_when_neither_tool_exists() {
        assert_eq!(choose_objcopy(false, false), None);
    }
}
