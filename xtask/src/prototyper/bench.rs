use std::process::ExitStatus;

use anyhow::Result;
use clap::Args;

use super::kernels::{self, Kernel};

/// Arguments for `cargo prototyper bench`.
#[derive(Debug, Args, Clone)]
pub struct BenchArgs {
    /// Package Prototyper and bench kernel into a single image
    #[arg(
        long,
        help = "Create a combined ITB image with a dynamic-mode Prototyper and the bench kernel"
    )]
    pub pack: bool,
}

pub(crate) fn run(args: &BenchArgs) -> Result<ExitStatus> {
    kernels::run(Kernel::Bench, args.pack)
}
