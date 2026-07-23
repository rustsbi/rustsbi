use std::ffi::OsString;
use std::process::ExitStatus;

use clap::Args;

#[derive(Debug, Args, Clone)]
pub struct TestArg {
    /// Package Prototyper and the S-mode test payload into one FIT image.
    #[clap(long)]
    pub pack: bool,
}

#[must_use]
pub fn run(arguments: &TestArg) -> Option<ExitStatus> {
    let mut forwarded = vec![
        OsString::from("build"),
        OsString::from("--image"),
        OsString::from("test"),
    ];
    if arguments.pack {
        forwarded.push("--pack".into());
    }
    crate::devkit::run(forwarded)
}
