mod build;
mod config;
mod generate;
mod kernels;
mod qemu;
mod scheme;
mod target;

#[cfg(test)]
mod tests;

use clap::Subcommand;
use std::process::ExitStatus;

use anyhow::Result;

pub(crate) const PACKAGE_NAME: &str = "rustsbi-prototyper";

/// Prototyper commands. `build` produces firmware; `test` and `bench`
/// compose a kernel build with a payload-mode firmware build, then boot
/// the firmware in QEMU and verify the kernel output (unless `--no-run`).
#[derive(Debug, Subcommand, Clone)]
pub enum PrototyperCommand {
    /// Build RustSBI Prototyper firmware.
    Build(build::BuildArgs),
    /// Build the test kernel and payload-mode firmware embedding it, then run it in QEMU.
    Test(kernels::KernelArgs),
    /// Build the bench kernel and payload-mode firmware embedding it, then run it in QEMU.
    Bench(kernels::KernelArgs),
}

pub use build::BuildArgs;
#[cfg(test)]
pub(crate) use build::BuildMode;
pub(crate) use kernels::Kernel;
pub(crate) use target::Target;

pub fn run(command: &PrototyperCommand) -> Result<ExitStatus> {
    match command {
        PrototyperCommand::Build(build_args) => build::run(build_args),
        PrototyperCommand::Test(args) => kernels::run(Kernel::Test, args),
        PrototyperCommand::Bench(args) => kernels::run(Kernel::Bench, args),
    }
}

#[cfg(test)]
pub(crate) use config::{PlatformAddresses, resolve_in};
#[cfg(test)]
pub(crate) use generate::{BuildPaths, generate_build_inputs, render_linker_script};
