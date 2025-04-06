use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use std::process::ExitCode;

#[macro_use]
mod utils;
mod bench;
mod logger;
mod prototyper;
mod test;

#[macro_use]
extern crate log;

use crate::bench::BenchArg;
use crate::prototyper::PrototyperArg;
use crate::test::TestArg;

#[derive(Parser)]
#[clap(
    name = "xtask",
    about = "A task runner for building, running and testing Prototyper",
    long_about = None,
)]
struct Cli {
    #[clap(subcommand)]
    cmd: Cmd,
    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build and configure the RustSBI Prototyper bootloader.
    Prototyper(PrototyperArg),
    /// Build test-kernel for the RustSBI Prototyper.
    Test(TestArg),
    /// Build bench-kernel for the RustSBI Prototyper.
    Bench(BenchArg),
}

fn main() -> ExitCode {
    let cli_args = Cli::parse();
    if let Err(e) = logger::Logger::init(&cli_args) {
        eprintln!("Logger initialization failed: {}", e);
        return ExitCode::FAILURE;
    }

    // Execute the selected command
    let result = match &cli_args.cmd {
        Cmd::Prototyper(arg) => prototyper::run(arg),
        Cmd::Test(arg) => test::run(arg),
        Cmd::Bench(arg) => bench::run(arg),
    };

    match result {
        Some(exit_status) if exit_status.success() => {
            info!("Task completed successfully");
            ExitCode::SUCCESS
        }
        Some(exit_status) => {
            let cmd_name = match &cli_args.cmd {
                Cmd::Prototyper(_) => "prototyper",
                Cmd::Test(_) => "test",
                Cmd::Bench(_) => "bench",
            };
            error!("Task '{}' failed with exit code: {}", cmd_name, exit_status);
            ExitCode::FAILURE
        }
        None => {
            error!(
                "Task execution failed: operation was interrupted or encountered an unrecoverable error"
            );
            ExitCode::FAILURE
        }
    }
}
