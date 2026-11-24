mod build;
mod config;
mod generate;
mod kernels;
mod qemu;
mod target;

#[cfg(test)]
mod tests;

use clap::Subcommand;

use anyhow::Result;

pub(crate) const PACKAGE_NAME: &str = "rustsbi-prototyper";

/// Prototyper commands. `build` produces firmware; `test` and `bench`
/// build `firmware/mbench` with config-derived addresses, run it in QEMU,
/// and verify the capture with `mbench` (unless `--no-run`).
#[derive(Debug, Subcommand, Clone)]
pub enum PrototyperCommand {
    /// Build RustSBI Prototyper firmware.
    Build(build::BuildArgs),
    /// Build `mbench` in check mode with Prototyper, run it in QEMU, and verify compliance.
    Test(kernels::KernelArgs),
    /// Build `mbench` in bench mode with Prototyper, run it in QEMU, and verify benchmarks.
    Bench(kernels::KernelArgs),
}

#[cfg(test)]
pub(crate) use build::{BuildArgs, BuildMode};
pub(crate) use target::Target;

pub fn run(command: &PrototyperCommand) -> Result<()> {
    match command {
        PrototyperCommand::Build(build_args) => {
            let status = build::run(build_args)?;
            if status.success() {
                Ok(())
            } else {
                anyhow::bail!("prototyper build exited with status {status}")
            }
        }
        PrototyperCommand::Test(args) => kernels::run(kernels::Kernel::Test, args),
        PrototyperCommand::Bench(args) => kernels::run(kernels::Kernel::Bench, args),
    }
}

#[cfg(test)]
pub(crate) use config::{FirmwareLayout, resolve_in};
#[cfg(test)]
pub(crate) use generate::{BuildPaths, generate_build_inputs, render_linker_script};
