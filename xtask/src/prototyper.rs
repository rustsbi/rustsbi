use std::{
    env, fs,
    process::{Command, ExitStatus},
};

use clap::Args;

use crate::utils::CmdOptional;
use crate::utils::cargo;

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

    #[clap(long, default_value = "INFO")]
    pub log_level: String,
}

#[must_use]
#[rustfmt::skip] // "export_env!("PROTOTYPER_FDT_PATH" ?= fdt.unwrap());" is a macro, rustfmt will not format it correctly
pub fn run(arg: &PrototyperArg) -> Option<ExitStatus> {
    let arch = "riscv64imac-unknown-none-elf";
    let fdt = arg.fdt.clone();
    let payload = arg.payload.clone();
    let jump = arg.jump;
    let current_dir = env::current_dir();
    let target_dir = current_dir
        .as_ref()
        .unwrap()
        .join("target")
        .join(arch)
        .join("release");

    info!("Building Protoyper");
    cargo::Cargo::new("build")
        .package("rustsbi-prototyper")
        .target(arch)
        .unstable("build-std", ["core","alloc"])
        .env("RUSTFLAGS", "-C relocation-model=pie -C link-arg=-pie")
        .features(&arg.features)
        .optional(arg.fdt.is_some(), |cargo| {
            cargo.env("PROTOTYPER_FDT_PATH", fdt.as_ref().unwrap());
            cargo.features(["fdt".to_string()])
        })
        .optional(payload.is_some(), |cargo| {
            cargo.env("PROTOTYPER_PAYLOAD_PATH", payload.as_ref().unwrap());
            cargo.features(["payload".to_string()])
        })
        .optional(jump, |cargo| {
            cargo.features(["jump".to_string()])
        })
        .env("RUST_LOG", &arg.log_level)
        .release()
        .status()
        .ok()?;

    info!("Copy to binary");
    let exit_status = Command::new("rust-objcopy")
        .args(["-O", "binary"])
        .arg("--binary-architecture=riscv64")
        .arg(target_dir.join("rustsbi-prototyper"))
        .arg(target_dir.join("rustsbi-prototyper.bin"))
        .status()
        .ok()?;
    if !exit_status.success() {
        error!("Failed to exec rust-objcopy, please check if cargo-binutils has been installed?");
        return Some(exit_status);
    }

    if arg.payload.is_some() {
        info!("Copy for payload mode");
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
    } else if arg.jump {
        info!("Copy for jump mode");
        fs::copy(
            target_dir.join("rustsbi-prototyper"),
            target_dir.join("rustsbi-prototyper-jump.elf"),
        )
        .ok()?;
        fs::copy(
            target_dir.join("rustsbi-prototyper.bin"),
            target_dir.join("rustsbi-prototyper-jump.bin"),
        )
        .ok()?;
    } else {
        info!("Copy for dynamic mode");
        fs::copy(
            target_dir.join("rustsbi-prototyper"),
            target_dir.join("rustsbi-prototyper-dynamic.elf"),
        )
        .ok()?;
        fs::copy(
            target_dir.join("rustsbi-prototyper.bin"),
            target_dir.join("rustsbi-prototyper-dynamic.bin"),
        )
        .ok()?;

    }

    Some(exit_status)
}
