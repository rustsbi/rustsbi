use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use crate::utils::{CmdOptional, cargo};
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

    #[clap(long, short = 'c')]
    pub config_file: Option<PathBuf>,

    #[clap(long)]
    pub target: Option<String>,
}

const ARCH: &str = "riscv64gc-unknown-none-elf";
const PACKAGE_NAME: &str = "rustsbi-prototyper";

#[must_use]
pub fn run(arg: &PrototyperArg) -> Option<ExitStatus> {
    let dirs = prepare_directories(arg)?;
    setup_config_file(&dirs.target_config_toml, arg)?;

    let exit_status = build_prototyper(arg)?;
    if !exit_status.success() {
        error!(
            "Failed to execute rust-objcopy. Please ensure that cargo-binutils is installed and available in your system's PATH."
        );
        return Some(exit_status);
    }

    copy_output_files(&dirs.target_dir, arg)?;

    Some(exit_status)
}

struct Directories {
    target_dir: PathBuf,
    target_config_toml: PathBuf,
}

fn prepare_directories(arg: &PrototyperArg) -> Option<Directories> {
    // The Rustc compiler gets target triple from the `--target` argument using `file_stem`
    // or the raw target name. Ref: compiler\rustc_target\src\spec\mod.rs of the Rust source code.
    fn get_target_triple(target: &str) -> String {
        fn is_target_file(target: &str) -> bool {
            target.ends_with(".json") && Path::new(target).exists()
        }
        if is_target_file(target) {
            Path::new(target)
                .file_stem()
                .and_then(|name| name.to_str())
                .ok_or_else(|| format!("Invalid file path: {}", target))
                .unwrap_or_else(|err| {
                    eprintln!("Warning: {}. Falling back to target string.", err);
                    target
                })
                .to_string()
        } else {
            target.to_string()
        }
    }

    let current_dir = env::current_dir().ok()?;
    let raw_target_dir = current_dir.join("target");
    let arch = arg.target.as_deref().unwrap_or(ARCH);
    let target_triple = get_target_triple(arch);
    let target_dir = raw_target_dir.join(target_triple).join("release");
    let target_config_toml = raw_target_dir.join("config.toml");

    Some(Directories {
        target_dir,
        target_config_toml,
    })
}

fn setup_config_file(target_config_toml: &PathBuf, arg: &PrototyperArg) -> Option<()> {
    // Delete old config if exists
    if fs::exists(target_config_toml).ok()? {
        info!("Delete old config");
        fs::remove_file(target_config_toml).ok()?;
    }

    // Determine config file path
    let current_dir = env::current_dir().ok()?;
    let default_config_file = current_dir
        .join("prototyper")
        .join("prototyper")
        .join("config")
        .join("default.toml");
    let config_file = arg.config_file.clone().unwrap_or(default_config_file);

    // Copy config
    info!("Copy config from: {}", config_file.display());
    fs::copy(&config_file, target_config_toml).ok()?;

    Some(())
}

fn build_prototyper(arg: &PrototyperArg) -> Option<ExitStatus> {
    info!("Building Prototyper");

    let enable_h = arg.features.iter().any(|f| f == "hypervisor");
    let rustflags = if enable_h {
        "-C relocation-model=pie -C link-arg=-pie -C target-feature=+h"
    } else {
        "-C relocation-model=pie -C link-arg=-pie"
    };

    let arch = arg.target.as_deref().unwrap_or(ARCH);

    // Build the prototyper
    let status = cargo::Cargo::new("build")
        .package(PACKAGE_NAME)
        .target(arch)
        .unstable("build-std", ["core", "alloc"])
        .env("RUSTFLAGS", rustflags)
        .features(&arg.features)
        .optional(arg.fdt.is_some(), |cargo| {
            cargo.env("PROTOTYPER_FDT_PATH", arg.fdt.as_ref().unwrap());
            cargo.features(["fdt".to_string()])
        })
        .optional(arg.payload.is_some(), |cargo| {
            cargo.env("PROTOTYPER_PAYLOAD_PATH", arg.payload.as_ref().unwrap());
            cargo.features(["payload".to_string()])
        })
        .optional(arg.jump, |cargo| cargo.features(["jump".to_string()]))
        .release()
        .status()
        .ok()?;

    if !status.success() {
        error!(
            "Failed to build prototyper. Please check the cargo output above for detailed error information."
        );
        return Some(status);
    }

    // Get target directory once instead of recreating it
    let target_dir = prepare_directories(arg)?.target_dir;
    let elf_path = target_dir.join(PACKAGE_NAME);
    let bin_path = target_dir.join(format!("{}.bin", PACKAGE_NAME));

    // Create binary from ELF
    info!("Converting ELF to binary with rust-objcopy");
    let result = Command::new("rust-objcopy")
        .args([
            "-O",
            "binary",
            "--binary-architecture=riscv64",
            &elf_path.to_string_lossy(),
            &bin_path.to_string_lossy(),
        ])
        .status()
        .ok();

    if result.is_none() {
        error!(
            "Failed to execute rust-objcopy. Command not found or failed to start.\n\
            Source: {}\n\
            Destination: {}\n\
            Please install cargo-binutils with cmd: cargo install cargo-binutils",
            elf_path.display(),
            bin_path.display()
        );
    }

    result
}

fn copy_output_files(target_dir: &PathBuf, arg: &PrototyperArg) -> Option<()> {
    let mode_suffix = if arg.payload.is_some() {
        info!("Copy for payload mode");
        "payload"
    } else if arg.jump {
        info!("Copy for jump mode");
        "jump"
    } else {
        info!("Copy for dynamic mode");
        "dynamic"
    };

    // Copy ELF file
    let elf_source = target_dir.join(PACKAGE_NAME);
    let elf_dest = target_dir.join(format!("{}-{}.elf", PACKAGE_NAME, mode_suffix));
    info!(
        "Copying ELF file: {} -> {}",
        elf_source.display(),
        elf_dest.display()
    );
    fs::copy(&elf_source, &elf_dest).ok()?;

    // Copy binary file
    let bin_source = target_dir.join(format!("{}.bin", PACKAGE_NAME));
    let bin_dest = target_dir.join(format!("{}-{}.bin", PACKAGE_NAME, mode_suffix));
    info!(
        "Copying binary file: {} -> {}",
        bin_source.display(),
        bin_dest.display()
    );
    fs::copy(&bin_source, &bin_dest).ok()?;

    Some(())
}
