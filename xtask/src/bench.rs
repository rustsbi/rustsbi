use std::ffi::OsString;
use std::process::ExitStatus;

use clap::Args;

#[derive(Debug, Args, Clone)]
pub struct BenchArg {
    /// Package Prototyper and the S-mode benchmark payload into one FIT image.
    #[clap(long)]
    pub pack: bool,
}

#[must_use]
pub fn run(arguments: &BenchArg) -> Option<ExitStatus> {
    let mut forwarded = vec![
        OsString::from("build"),
        OsString::from("--image"),
        OsString::from("bench"),
    ];
    if arguments.pack {
        forwarded.push("--pack".into());
    }
    crate::devkit::run(forwarded)
}
