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
    Prototyper(PrototyperArg),
    Test(TestArg),
    Bench(BenchArg),
    Qemu(PrototyperArg),
}

fn main() -> ExitCode {
    let cli_args = Cli::parse();
    logger::Logger::init(&cli_args).expect("Unable to init logger");

    if let Some(code) = match cli_args.cmd {
        Cmd::Prototyper(ref arg) => prototyper::run(arg),
        Cmd::Test(ref arg) => test::run(arg),
        Cmd::Bench(ref arg) => bench::run(arg),
        Cmd::Qemu(ref arg) => prototyper::qemu_run(arg),
    } {
        if code.success() {
            info!("Finished");
            return ExitCode::SUCCESS;
        }
    }

    error!("Failed to run task!");
    ExitCode::FAILURE
}
