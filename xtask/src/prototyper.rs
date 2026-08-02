use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitStatus;

use clap::Args;

#[derive(Debug, Args, Clone)]
pub struct PrototyperArg {
    #[clap(long, short = 'f')]
    pub features: Vec<String>,

    #[clap(long, env = "PROTOTYPER_FDT_PATH")]
    pub fdt: Option<String>,

    #[clap(long, env = "PROTOTYPER_PAYLOAD_PATH")]
    pub payload: Option<String>,

    #[clap(long)]
    pub jump: bool,

    #[clap(long)]
    pub debug: bool,

    #[clap(long, short = 'c')]
    pub config_file: Option<PathBuf>,

    #[clap(long)]
    pub target: Option<String>,
}

#[must_use]
pub fn run(arguments: &PrototyperArg) -> Option<ExitStatus> {
    let mut forwarded = vec![OsString::from("build")];
    for feature in &arguments.features {
        forwarded.extend([OsString::from("--features"), feature.into()]);
    }
    push_value(&mut forwarded, "--fdt", arguments.fdt.as_deref());
    push_value(&mut forwarded, "--payload", arguments.payload.as_deref());
    push_value(&mut forwarded, "--target", arguments.target.as_deref());
    if let Some(config) = &arguments.config_file {
        forwarded.extend([OsString::from("--config-file"), config.as_os_str().into()]);
    }
    if arguments.jump {
        forwarded.push("--jump".into());
    }
    if arguments.debug {
        forwarded.push("--debug".into());
    }
    crate::devkit::run(forwarded)
}

fn push_value(arguments: &mut Vec<OsString>, option: &str, value: Option<&str>) {
    if let Some(value) = value {
        arguments.extend([option.into(), value.into()]);
    }
}
