use std::{
    env,
    process::{Command, ExitStatus},
};

use clap::Args;

use crate::utils::cargo;
use crate::utils::CmdOptional;

#[derive(Debug, Args, Clone)]
pub struct PrototyperArg {
    #[clap(long, short = 'f')]
    pub features: Vec<String>,

    #[clap(long, env = "PROTOTYPER_FDT_PATH")]
    pub fdt: Option<String>,

    #[clap(long, env = "PROTOTYPER_PAYLOAD_PATH")]
    pub payload: Option<String>,
}

#[must_use]
pub fn run(arg: &PrototyperArg) -> Option<ExitStatus> {
    let arch = "riscv64imac-unknown-none-elf";
    let fdt = arg.fdt.clone();
    let payload = arg.payload.clone();

    cargo::Cargo::new("build")
        .package("rustsbi-prototyper")
        .target(arch)
        .unstable("build-std", ["core"])
        .env("RUSTFLAGS", "-C relocation-model=pie -C link-arg=-pie")
        .features(&arg.features)
        .optional(arg.fdt.is_some(), |cargo| {
            export_env!("PROTOTYPER_FDT_PATH" ?= fdt.unwrap());
            cargo.features(["fdt".to_string()])
        })
        .optional(payload.is_some(), |cargo| {
            export_env!("PROTOTYPER_PAYLOAD_PATH" ?= payload.unwrap());
            cargo.features(["payload".to_string()])
        })
        .release()
        .status()
        .ok()?;

    Command::new("rust-objcopy")
        .args(["-O", "binary"])
        .arg("--binary-architecture=riscv64")
        .arg(
            env::current_dir()
                .unwrap()
                .join("target")
                .join(arch)
                .join("release")
                .join("rustsbi-prototyper"),
        )
        .arg(
            env::current_dir()
                .unwrap()
                .join("target")
                .join(arch)
                .join("release")
                .join("rustsbi-prototyper.bin"),
        )
        .status()
        .ok()
}
