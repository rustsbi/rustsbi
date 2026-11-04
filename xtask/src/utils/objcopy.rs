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

/// Convert an ELF to binary, using LLVM if rust-objcopy is missing.
pub(crate) fn convert_elf_to_binary(
    elf: &Path,
    binary: &Path,
) -> Result<(&'static str, ExitStatus)> {
    let args = [
        OsStr::new("-O"),
        OsStr::new("binary"),
        elf.as_os_str(),
        binary.as_os_str(),
    ];

    match run_tool(RUST_OBJCOPY, &args) {
        Ok(status) => return Ok((RUST_OBJCOPY, status)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => {}
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to execute {RUST_OBJCOPY}: '{}' -> '{}'",
                    elf.display(),
                    binary.display()
                )
            });
        }
    }

    match run_tool(LLVM_OBJCOPY, &args) {
        Ok(status) => Ok((LLVM_OBJCOPY, status)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Err(err).with_context(|| {
            format!(
                "failed to execute {RUST_OBJCOPY} or {LLVM_OBJCOPY}: '{}' -> '{}'; \
                 install cargo-binutils with `cargo install --locked cargo-binutils@0.4.0`, \
                 or install llvm-objcopy",
                elf.display(),
                binary.display()
            )
        }),
        Err(err) => Err(err).with_context(|| {
            format!(
                "failed to execute {LLVM_OBJCOPY} ({RUST_OBJCOPY} was not found): '{}' -> '{}'",
                elf.display(),
                binary.display()
            )
        }),
    }
}

fn run_tool(tool: &str, args: &[&OsStr]) -> io::Result<ExitStatus> {
    let mut child = Command::new(tool).args(args).spawn()?;
    info!("Using {tool} to convert ELF to binary");
    child.wait()
}
