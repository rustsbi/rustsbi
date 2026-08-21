use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::Duration,
};

use crate::utils::{cargo, cargo_target_dir, workspace_root};
use anyhow::{Context, Result, bail};

use super::{
    PACKAGE_NAME,
    build::{BuildArgs, build_firmware},
    config::resolve,
    qemu::{self, QemuRun},
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

    /// Default number of harts QEMU boots this kernel with.
    pub(super) fn default_smp(self) -> usize {
        match self {
            Kernel::Test => 1,
            Kernel::Bench => 4,
        }
    }

    /// Default timeout of one QEMU attempt, in seconds.
    pub(super) fn default_timeout_secs(self) -> u64 {
        match self {
            Kernel::Test => 60,
            Kernel::Bench => 90,
        }
    }

    /// Default number of QEMU attempts; retries happen only after a timeout.
    pub(super) fn default_attempts(self) -> usize {
        match self {
            Kernel::Test => 2,
            Kernel::Bench => 4,
        }
    }

    /// Console output patterns expected from a successful run of this kernel.
    ///
    /// Read from the kernel's `scripts/expected.txt` — the single source
    /// shared with `.github/scripts/prototyper-qemu-boot.sh`, which verifies
    /// the same kernels in dynamic and jump mode. `{smp}` placeholders are
    /// replaced with the hart count.
    pub(super) fn expected_patterns(self, smp: usize) -> Result<Vec<String>> {
        let path = workspace_root()
            .join("prototyper")
            .join(self.dir_name())
            .join("scripts")
            .join("expected.txt");
        Ok(read_console_patterns(&path)?
            .into_iter()
            .map(|pattern| pattern.replace("{smp}", &smp.to_string()))
            .collect())
    }

    /// Build this kernel for the `imac` target and convert it to raw binary.
    ///
    /// Returns the path of the produced `.bin`, used as the payload input of
    /// the firmware build.
    fn build(self) -> Result<PathBuf> {
        let (_, target_dir) = kernel_paths();

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
    ///
    fn pack(self, firmware_options: &FirmwareOptions) -> Result<()> {
        let (workspace_root, target_dir) = kernel_paths();

        info!("Building dynamic firmware for packing");
        let dynamic_spec = resolve(&BuildArgs::dynamic(
            firmware_options.debug,
            firmware_options.config_file.clone(),
        ))
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
            .artifact_dir()
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

        let its_source = workspace_root
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

/// QEMU execution options shared by kernel-backed commands.
#[derive(Debug, Clone, Copy)]
pub(super) struct QemuOptions {
    /// Only build the kernel and firmware without running them in QEMU.
    pub no_run: bool,
    /// Number of harts QEMU boots the kernel with.
    pub smp: usize,
    /// Timeout of one QEMU attempt, in seconds.
    pub timeout_secs: u64,
    /// Number of QEMU attempts; retries happen only after a timeout.
    pub attempts: usize,
}

impl QemuOptions {
    /// Reject nonsensical values before anything is built, so `--no-run`
    /// invocations fail fast too. `qemu::run` repeats these checks as
    /// defense in depth.
    pub(super) fn validate(&self) -> Result<()> {
        if self.attempts == 0 {
            bail!("QEMU attempts must be at least 1 (got --retries 0)");
        }
        if self.smp == 0 {
            bail!("QEMU hart count must be at least 1 (got --smp 0)");
        }
        Ok(())
    }
}

/// Firmware build options shared by kernel-backed commands, forwarded to
/// the payload-mode firmware build.
#[derive(Debug, Clone, Default)]
pub(super) struct FirmwareOptions {
    /// Build the firmware in the debug profile instead of release.
    pub debug: bool,
    /// Custom firmware config file.
    pub config_file: Option<PathBuf>,
}

/// Run a kernel-backed prototyper command (`test` or `bench`):
/// build the kernel and the payload-mode firmware embedding it, then boot
/// the firmware in QEMU and verify the kernel's console output.
pub(super) fn run(
    kernel: Kernel,
    pack: bool,
    qemu_options: QemuOptions,
    firmware_options: &FirmwareOptions,
) -> Result<ExitStatus> {
    qemu_options.validate()?;

    let kernel_binary = kernel.build()?;
    let build_args = BuildArgs::payload(
        kernel_binary,
        firmware_options.debug,
        firmware_options.config_file.clone(),
    );
    let mut spec = resolve(&build_args).context("failed to resolve prototyper build inputs")?;
    spec.override_artifact_suffix(format!("payload-{}", kernel.command_name()));

    let exit_status = build_firmware(&spec)?;
    if !exit_status.success() {
        return Ok(exit_status);
    }

    if pack {
        kernel.pack(firmware_options)?;
    }

    if !qemu_options.no_run {
        let firmware_elf = spec
            .artifact_dir()
            .join(format!("{PACKAGE_NAME}-{}.elf", spec.artifact_suffix()));
        qemu::run(&QemuRun {
            bios: firmware_elf,
            smp: qemu_options.smp,
            timeout: Duration::from_secs(qemu_options.timeout_secs),
            attempts: qemu_options.attempts,
            expected: kernel.expected_patterns(qemu_options.smp)?,
            forbidden: forbidden_patterns()?,
            label: kernel.command_name().to_string(),
        })?;
    }

    Ok(exit_status)
}

/// Paths used by kernel builds: the workspace root (for kernel scripts) and
/// the kernel's cargo target directory (honors `CARGO_TARGET_DIR`).
fn kernel_paths() -> (PathBuf, PathBuf) {
    (
        workspace_root(),
        cargo_target_dir().join(ARCH).join("release"),
    )
}

/// Console substrings that mark a failed QEMU boot even when the process
/// exits successfully, read from `prototyper/scripts/qemu-forbidden.txt` —
/// the single source shared with `.github/scripts/prototyper-qemu-boot.sh`.
pub(super) fn forbidden_patterns() -> Result<Vec<String>> {
    read_console_patterns(
        &workspace_root()
            .join("prototyper")
            .join("scripts")
            .join("qemu-forbidden.txt"),
    )
}

/// Read console patterns from a pattern file: one fixed string per line,
/// skipping blank lines and `#` comments.
fn read_console_patterns(path: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read console pattern file '{}'", path.display()))?;
    Ok(content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect())
}
