use clap::{Parser, Subcommand};
use clap_verbosity_flag::{InfoLevel, Verbosity};
use std::process::ExitCode;

#[macro_use]
mod utils;
mod logger;
mod prototyper;

#[macro_use]
extern crate log;

use crate::prototyper::PrototyperCommand;

#[derive(Parser)]
#[command(
    name = "xtask",
    about = "A task runner for building, running and testing Prototyper",
    long_about = None,
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Build and configure the RustSBI Prototyper bootloader.
    Prototyper {
        #[command(subcommand)]
        command: PrototyperCommand,
    },
}

fn main() -> ExitCode {
    let cli_args = Cli::parse();
    if let Err(e) = logger::Logger::init(&cli_args) {
        eprintln!("Logger initialization failed: {}", e);
        return ExitCode::FAILURE;
    }

    // Execute the selected command
    let result = match &cli_args.cmd {
        Cmd::Prototyper { command } => match prototyper::run(command) {
            Ok(exit_status) => Some(exit_status),
            Err(err) => {
                error!("Task 'prototyper' failed: {:#}", err);
                return ExitCode::FAILURE;
            }
        },
    };

    match result {
        Some(exit_status) if exit_status.success() => {
            info!("Task completed successfully");
            ExitCode::SUCCESS
        }
        Some(exit_status) => {
            let cmd_name = match &cli_args.cmd {
                Cmd::Prototyper { .. } => "prototyper",
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
