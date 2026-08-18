mod bench;
mod build;
mod config;
mod generate;
mod kernels;
mod test;

#[cfg(test)]
mod tests;

use clap::Subcommand;
use std::process::ExitStatus;

use anyhow::Result;

pub(crate) const ARCH: &str = "riscv64gc-unknown-none-elf";
pub(crate) const PACKAGE_NAME: &str = "rustsbi-prototyper";

/// Prototyper commands. `build` produces firmware; `test` and `bench`
/// compose a kernel build with a payload-mode firmware build.
#[derive(Debug, Subcommand, Clone)]
pub enum PrototyperCommand {
    /// Build RustSBI Prototyper firmware.
    Build(build::BuildArgs),
    /// Build the test kernel and payload-mode firmware embedding it.
    Test(test::TestArgs),
    /// Build the bench kernel and payload-mode firmware embedding it.
    Bench(bench::BenchArgs),
}

pub use build::BuildArgs;
#[cfg(test)]
pub(crate) use build::BuildMode;

pub fn run(command: &PrototyperCommand) -> Result<ExitStatus> {
    match command {
        PrototyperCommand::Build(build_args) => build::run(build_args),
        PrototyperCommand::Test(test_args) => test::run(test_args),
        PrototyperCommand::Bench(bench_args) => bench::run(bench_args),
    }
}

#[cfg(test)]
pub(crate) use config::{PlatformAddresses, resolve_in};
#[cfg(test)]
pub(crate) use generate::{BuildPaths, generate_build_inputs, render_linker_script};
