use std::process::ExitStatus;

use anyhow::Result;
use clap::Args;

use super::kernels::{self, Kernel};

/// Arguments for `cargo prototyper test`.
#[derive(Debug, Args, Clone)]
pub struct TestArgs {
    /// Package Prototyper and Test-Kernel into a single image
    #[arg(
        long,
        help = "Create a combined ITB image with a dynamic-mode Prototyper and the test kernel"
    )]
    pub pack: bool,
}

pub(crate) fn run(args: &TestArgs) -> Result<ExitStatus> {
    kernels::run(Kernel::Test, args.pack)
}
