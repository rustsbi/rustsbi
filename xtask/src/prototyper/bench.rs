use std::path::PathBuf;
use std::process::ExitStatus;

use anyhow::Result;
use clap::Args;

use super::kernels::{self, FirmwareOptions, Kernel, QemuOptions};

/// Arguments for `cargo prototyper bench`.
#[derive(Debug, Args, Clone)]
pub struct BenchArgs {
    /// Package Prototyper and bench kernel into a single image
    #[arg(
        long,
        help = "Create a combined ITB image with a dynamic-mode Prototyper and the bench kernel"
    )]
    pub pack: bool,

    /// Only build the bench kernel and firmware without running them in QEMU
    #[arg(long)]
    pub no_run: bool,

    /// Number of harts QEMU boots the bench kernel with
    #[arg(long, default_value_t = Kernel::Bench.default_smp())]
    pub smp: usize,

    /// Timeout in seconds of one QEMU attempt
    #[arg(long, default_value_t = Kernel::Bench.default_timeout_secs())]
    pub timeout: u64,

    /// Number of QEMU attempts; retries happen only after a timeout
    #[arg(long, default_value_t = Kernel::Bench.default_attempts())]
    pub retries: usize,

    /// Build the firmware in the debug profile instead of release
    #[arg(long)]
    pub debug: bool,

    /// Specify the path to a custom configuration file for the firmware
    #[arg(long, short = 'c')]
    pub config_file: Option<PathBuf>,
}

pub(crate) fn run(args: &BenchArgs) -> Result<ExitStatus> {
    kernels::run(
        Kernel::Bench,
        args.pack,
        QemuOptions {
            no_run: args.no_run,
            smp: args.smp,
            timeout_secs: args.timeout,
            attempts: args.retries,
        },
        &FirmwareOptions {
            debug: args.debug,
            config_file: args.config_file.clone(),
        },
    )
}
