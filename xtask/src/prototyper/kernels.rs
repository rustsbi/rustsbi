use std::{
    env, fs,
    path::PathBuf,
    process::{Command, ExitStatus},
};

use crate::utils::cargo;
use anyhow::{Context, Result, bail};

use super::{
    PACKAGE_NAME,
    build::{BuildArgs, build_firmware},
    config::resolve,
};

/// Kernels the prototyper can embed as payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kernel {
    Test,
    Bench,
}

const ARCH: &str = "riscv64imac-unknown-none-elf";
const PROTOTYPER_BIN: &str = "rustsbi-prototyper.bin";

impl Kernel {
    /// Cargo package name of the kernel.
    fn package_name(self) -> &'static str {
        match self {
            Kernel::Test => "rustsbi-test-kernel",
            Kernel::Bench => "rustsbi-bench-kernel",
        }
    }

    /// Directory under `prototyper/` holding this kernel's scripts.
    fn dir_name(self) -> &'static str {
        match self {
            Kernel::Test => "test-kernel",
            Kernel::Bench => "bench-kernel",
        }
    }

    /// Name used by the `cargo prototyper` command and artifact suffix.
    fn command_name(self) -> &'static str {
        match self {
            Kernel::Test => "test",
            Kernel::Bench => "bench",
        }
    }

    /// Build this kernel for the `imac` target and convert it to raw binary.
    ///
    /// Returns the path of the produced `.bin`, used as the payload input of
    /// the firmware build.
    fn build(self) -> Result<PathBuf> {
        let (_, target_dir) = kernel_paths()?;

        info!("Building {} kernel", self.command_name());
        let build_status = cargo::Cargo::new("build")
            .package(self.package_name())
            .target(ARCH)
            .release()
            .status()
            .with_context(|| {
                format!(
                    "failed to execute cargo build for package '{}' with target '{}'",
                    self.package_name(),
                    ARCH
                )
            })?;
        if !build_status.success() {
            bail!(
                "failed to build {} kernel; please check the cargo output above for detailed error information",
                self.command_name()
            );
        }

        info!("Converting {} kernel to binary format", self.command_name());
        let kernel_path = target_dir.join(self.package_name());
        let bin_path = target_dir.join(format!("{}.bin", self.package_name()));
        let objcopy_status = Command::new("rust-objcopy")
            .args([
                "-O",
                "binary",
                "--binary-architecture=riscv64",
                &kernel_path.to_string_lossy(),
                &bin_path.to_string_lossy(),
            ])
            .status()
            .with_context(|| {
                format!(
                    "failed to execute rust-objcopy: '{}' -> '{}'; \
                     please install cargo-binutils with cmd: cargo install cargo-binutils",
                    kernel_path.display(),
                    bin_path.display()
                )
            })?;
        if !objcopy_status.success() {
            bail!(
                "rust-objcopy failed to convert the {} kernel ELF to binary",
                self.command_name()
            );
        }

        info!("Output binary created at: {}", bin_path.display());
        Ok(bin_path)
    }

    /// Pack this kernel and a dynamic-mode Prototyper binary into a
    /// single mkimage ITB, driven by the kernel's ITS file.
    ///
    /// The ITB carries the kernel as a separate FIT loadable, so the packed
    /// firmware must not embed it: build a fresh dynamic-mode firmware and
    /// stage its unsuffixed intermediate `rustsbi-prototyper.bin` beside the
    /// kernel, where the kernel's ITS expects it.
    fn pack(self) -> Result<()> {
        let (current_dir, target_dir) = kernel_paths()?;

        info!("Building dynamic firmware for packing");
        let dynamic_spec = resolve(&BuildArgs::dynamic())
            .context("failed to resolve dynamic firmware build inputs for packing")?;
        let build_status = build_firmware(&dynamic_spec)?;
        if !build_status.success() {
            bail!(
                "failed to build dynamic firmware for packing the {} kernel; \
                 please check the cargo output above for detailed error information",
                self.command_name()
            );
        }
        let firmware_bin = dynamic_spec
            .artifact_dir(&current_dir)
            .join(format!("{PACKAGE_NAME}.bin"));

        info!("Packing {} kernel into image", self.command_name());

        // Firmware and kernel use different targets; stage the firmware where
        // the kernel's ITS expects it.
        let prototyper_bin_path = target_dir.join(PROTOTYPER_BIN);
        fs::copy(&firmware_bin, &prototyper_bin_path).with_context(|| {
            format!(
                "failed to stage firmware binary: '{}' -> '{}'",
                firmware_bin.display(),
                prototyper_bin_path.display()
            )
        })?;

        let its_source = current_dir
            .join("prototyper")
            .join(self.dir_name())
            .join("scripts")
            .join(format!("{}.its", self.package_name()));

        let its_dest = target_dir.join(format!("{}.its", self.package_name()));

        fs::copy(&its_source, &its_dest).with_context(|| {
            format!(
                "failed to copy ITS file: '{}' -> '{}'",
                its_source.display(),
                its_dest.display()
            )
        })?;

        let status = Command::new("mkimage")
            .args([
                "-f",
                &format!("{}.its", self.package_name()),
                &format!("{}.itb", self.package_name()),
            ])
            .current_dir(&target_dir)
            .status()
            .context("failed to execute mkimage command")?;

        fs::remove_file(&its_dest)
            .with_context(|| format!("failed to clean up ITS file: '{}'", its_dest.display()))?;

        if !status.success() {
            bail!(
                "mkimage failed to pack the {} kernel image",
                self.command_name()
            );
        }

        info!(
            "Output image created at: {}",
            target_dir
                .join(format!("{}.itb", self.package_name()))
                .display()
        );
        Ok(())
    }
}

/// Run a kernel-backed prototyper command (`test` or `bench`).
pub(super) fn run(kernel: Kernel, pack: bool) -> Result<ExitStatus> {
    let kernel_binary = kernel.build()?;
    let build_args = BuildArgs::payload(kernel_binary, false);
    let mut spec = resolve(&build_args).context("failed to resolve prototyper build inputs")?;
    spec.override_artifact_suffix(format!("payload-{}", kernel.command_name()));

    let current_dir = env::current_dir().context("failed to determine current directory")?;

    let exit_status = build_firmware(&spec)?;
    if !exit_status.success() {
        return Ok(exit_status);
    }

    if pack {
        kernel.pack()?;
    }

    Ok(exit_status)
}

fn kernel_paths() -> Result<(PathBuf, PathBuf)> {
    let current_dir = env::current_dir().context("failed to determine current directory")?;
    let target_dir = current_dir.join("target").join(ARCH).join("release");
    Ok((current_dir, target_dir))
}
