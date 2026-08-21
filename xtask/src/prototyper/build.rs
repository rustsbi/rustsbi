use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};

use crate::utils::{CmdOptional, cargo};

use super::{
    PACKAGE_NAME,
    config::{BuildSpec, resolve},
    generate::{BuildPaths, generate_build_inputs, prepare_build_paths},
};

/// Arguments for `cargo prototyper build`.
#[derive(Debug, Args, Clone)]
pub struct BuildArgs {
    #[command(subcommand)]
    pub mode: Option<BuildMode>,

    #[arg(long, short = 'f')]
    pub features: Vec<String>,

    #[arg(long)]
    pub fdt: Option<PathBuf>,

    #[arg(long)]
    pub debug: bool,

    #[arg(long, short = 'c')]
    pub config_file: Option<PathBuf>,

    #[arg(long)]
    pub target: Option<String>,
}

/// Firmware image variant selected by `cargo prototyper build`.
#[derive(Debug, Subcommand, Clone, PartialEq, Eq)]
pub enum BuildMode {
    /// Build dynamic firmware (default when no subcommand is given).
    Dynamic,
    /// Build jump-mode firmware.
    Jump,
    /// Build payload-mode firmware embedding the given payload binary.
    Payload {
        /// Path to the payload binary to embed.
        path: PathBuf,
    },
}

impl BuildArgs {
    pub(crate) fn payload(path: PathBuf, debug: bool) -> Self {
        Self {
            mode: Some(BuildMode::Payload { path }),
            features: Vec::new(),
            fdt: None,
            debug,
            config_file: None,
            target: None,
        }
    }

    /// Dynamic-mode firmware build with default options.
    pub(crate) fn dynamic() -> Self {
        Self {
            mode: None,
            features: Vec::new(),
            fdt: None,
            debug: false,
            config_file: None,
            target: None,
        }
    }
}

pub(crate) fn run(args: &BuildArgs) -> Result<ExitStatus> {
    let spec = resolve(args).context("failed to resolve prototyper build inputs")?;
    build_firmware(&spec)
}

/// Build firmware from a resolved specification.
pub(super) fn build_firmware(spec: &BuildSpec) -> Result<ExitStatus> {
    let paths = prepare_build_paths(spec)?;
    generate_build_inputs(spec, &paths)?;

    let exit_status = cargo_build(spec, &paths)?;
    if !exit_status.success() {
        error!(
            "Failed to build prototyper. Please check the cargo output above for detailed error information."
        );
        return Ok(exit_status);
    }

    let exit_status = convert_elf_to_binary(&paths)?;
    if !exit_status.success() {
        error!("rust-objcopy failed to convert the prototyper ELF to binary");
        return Ok(exit_status);
    }

    copy_mode_artifacts(spec, &paths)?;
    Ok(exit_status)
}

fn cargo_build(spec: &BuildSpec, paths: &BuildPaths) -> Result<ExitStatus> {
    info!("Building Prototyper");

    let linker_script = paths.linker_script_argument();
    let mut command = cargo::Cargo::new("build");
    command
        .package(PACKAGE_NAME)
        .target(&spec.target)
        .unstable("build-std", ["core", "alloc"])
        .env(
            "CARGO_ENCODED_RUSTFLAGS",
            spec.encoded_rustflags(&linker_script),
        )
        .features(spec.cargo_features())
        .optional(!spec.debug, |cargo| cargo.release());

    command.status().with_context(|| {
        format!(
            "failed to execute cargo build for package '{}' with target '{}'",
            PACKAGE_NAME, spec.target
        )
    })
}

fn convert_elf_to_binary(paths: &BuildPaths) -> Result<ExitStatus> {
    let elf_path = paths.artifact_dir.join(PACKAGE_NAME);
    let binary_path = paths.artifact_dir.join(format!("{PACKAGE_NAME}.bin"));

    info!("Converting ELF to binary with rust-objcopy");
    Command::new("rust-objcopy")
        .args([
            "-O",
            "binary",
            "--binary-architecture=riscv64",
            &elf_path.to_string_lossy(),
            &binary_path.to_string_lossy(),
        ])
        .status()
        .with_context(|| {
            format!(
                "failed to execute rust-objcopy: '{}' -> '{}'; \
                 please install cargo-binutils with cmd: cargo install cargo-binutils",
                elf_path.display(),
                binary_path.display()
            )
        })
}

fn copy_mode_artifacts(spec: &BuildSpec, paths: &BuildPaths) -> Result<()> {
    let mode_suffix = spec.artifact_suffix();
    info!("Copy artifacts for {} mode", mode_suffix);

    remove_stale_generic_payload_artifacts(&paths.artifact_dir, mode_suffix)?;

    let elf_source = paths.artifact_dir.join(PACKAGE_NAME);
    let elf_destination = paths
        .artifact_dir
        .join(format!("{PACKAGE_NAME}-{mode_suffix}.elf"));
    info!(
        "Copying ELF file: {} -> {}",
        elf_source.display(),
        elf_destination.display()
    );
    copy_artifact(&elf_source, &elf_destination)?;

    let binary_source = paths.artifact_dir.join(format!("{PACKAGE_NAME}.bin"));
    let binary_destination = paths
        .artifact_dir
        .join(format!("{PACKAGE_NAME}-{mode_suffix}.bin"));
    info!(
        "Copying binary file: {} -> {}",
        binary_source.display(),
        binary_destination.display()
    );
    copy_artifact(&binary_source, &binary_destination)
}

/// Remove the generic `rustsbi-prototyper-payload.{elf,bin}` artifacts when
/// building a kernel-suffixed payload variant (e.g. `payload-test`), so a
/// stale generic artifact cannot be mistaken for the fresh output. Dynamic
/// and jump artifacts are never touched: CI builds all modes side by side.
pub(super) fn remove_stale_generic_payload_artifacts(
    artifact_dir: &Path,
    mode_suffix: &str,
) -> Result<()> {
    if !(mode_suffix.starts_with("payload") && mode_suffix != "payload") {
        return Ok(());
    }
    for extension in ["elf", "bin"] {
        let stale = artifact_dir.join(format!("{PACKAGE_NAME}-payload.{extension}"));
        match fs::remove_file(&stale) {
            Ok(()) => info!("Removed stale payload artifact: {}", stale.display()),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to remove stale payload artifact '{}'",
                        stale.display()
                    )
                });
            }
        }
    }
    Ok(())
}

fn copy_artifact(source: &Path, destination: &Path) -> Result<()> {
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy artifact '{}' to '{}'",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}
