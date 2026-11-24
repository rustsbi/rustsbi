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
    match &cli_args.cmd {
        Cmd::Prototyper { command } => match prototyper::run(command) {
            Ok(()) => {
                info!("Task completed successfully");
                ExitCode::SUCCESS
            }
            Err(err) => {
                error!("Task 'prototyper' failed: {:#}", err);
                ExitCode::FAILURE
            }
        },
    }
}
