use std::process::ExitStatus;

use anyhow::Result;
use clap::Args;

use super::kernels::{self, Kernel, QemuOptions};

/// Arguments for `cargo prototyper test`.
#[derive(Debug, Args, Clone)]
pub struct TestArgs {
    /// Package Prototyper and Test-Kernel into a single image
    #[arg(
        long,
        help = "Create a combined ITB image with a dynamic-mode Prototyper and the test kernel"
    )]
    pub pack: bool,

    /// Only build the test kernel and firmware without running them in QEMU
    #[arg(long)]
    pub no_run: bool,

    /// Number of harts QEMU boots the test kernel with
    #[arg(long, default_value_t = Kernel::Test.default_smp())]
    pub smp: usize,

    /// Timeout in seconds of one QEMU attempt
    #[arg(long, default_value_t = Kernel::Test.default_timeout_secs())]
    pub timeout: u64,

    /// Number of QEMU attempts; retries happen only after a timeout
    #[arg(long, default_value_t = Kernel::Test.default_attempts())]
    pub retries: usize,
}

pub(crate) fn run(args: &TestArgs) -> Result<ExitStatus> {
    kernels::run(
        Kernel::Test,
        args.pack,
        QemuOptions {
            no_run: args.no_run,
            smp: args.smp,
            timeout_secs: args.timeout,
            attempts: args.retries,
        },
    )
}
