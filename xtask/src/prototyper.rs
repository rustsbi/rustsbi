use std::{
    env, fs,
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
#[rustfmt::skip] // "export_env!("PROTOTYPER_FDT_PATH" ?= fdt.unwrap());" is a macro, rustfmt will not format it correctly
pub fn run(arg: &PrototyperArg) -> Option<ExitStatus> {
    let arch = "riscv64imac-unknown-none-elf";
    let fdt = arg.fdt.clone();
    let payload = arg.payload.clone();
    let current_dir = env::current_dir();
    let target_dir = current_dir
        .as_ref()
        .unwrap()
        .join("target")
        .join(arch)
        .join("release");

    let status = cargo::Cargo::new("build")
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

    if status.success() {
        let exit_status = Command::new("rust-objcopy")
            .args(["-O", "binary"])
            .arg("--binary-architecture=riscv64")
            .arg(target_dir.join("rustsbi-prototyper"))
            .arg(target_dir.join("rustsbi-prototyper.bin"))
            .status()
            .ok()?;

        if arg.payload.is_some() {
            fs::copy(
                target_dir.join("rustsbi-prototyper"),
                target_dir.join("rustsbi-prototyper-payload.elf"),
            )
            .ok()?;
            fs::copy(
                target_dir.join("rustsbi-prototyper.bin"),
                target_dir.join("rustsbi-prototyper-payload.bin"),
            )
            .ok()?;
        } else {
            fs::copy(
                target_dir.join("rustsbi-prototyper"),
                target_dir.join("rustsbi-prototyper-dynamic.elf"),
            )
            .ok()?;
            fs::copy(
                target_dir.join("rustsbi-prototyper.bin"),
                target_dir.join("rustsbi-prototyper-dynamic.bin"),
            ).ok()?;
        }
        return Some(exit_status);
    } else {
        eprintln!("Build failed with status: {:?}", status);
        return Some(status);
    }
}
