use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::Duration,
};

use crate::utils::{cargo, cargo_target_dir, workspace_root};
use anyhow::{Context, Result, bail};
use clap::Args;

use super::{
    PACKAGE_NAME, Target,
    build::{BuildArgs, build_firmware},
    config::resolve,
    qemu::{self, QemuRun},
    scheme::{Action, Scheme},
};

impl From<Kernel> for Action {
    fn from(kernel: Kernel) -> Self {
        match kernel {
            Kernel::Test => Action::Test,
            Kernel::Bench => Action::Bench,
        }
    }
}

/// Kernels the prototyper can embed as payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Kernel {
    Test,
    Bench,
}

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

    /// Console output patterns expected from a successful run of this kernel.
    ///
    /// Read from the kernel's `scripts/expected.txt` — the single source
    /// shared with `.github/scripts/prototyper-qemu-boot.sh`, which verifies
    /// the same kernels in dynamic and jump mode. `{smp}` placeholders are
    /// replaced with the hart count.
    pub(super) fn expected_patterns(self, smp: usize) -> Result<Vec<String>> {
        let path = workspace_root()
            .join("firmware")
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
            .target(Target::Kernel.triple())
            .release()
            .status()
            .with_context(|| {
                format!(
                    "failed to execute cargo build for package '{}' with target '{}'",
                    self.package_name(),
                    Target::Kernel.triple()
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
                     please install cargo-binutils with cmd: cargo install --locked cargo-binutils@0.4.0",
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
    /// The pack build honors the user's forwarded firmware options, and its
    /// mode artifacts are written under the dedicated `dynamic-pack` suffix
    /// so a pre-built `-dynamic` (or any other mode) artifact is never
    /// silently replaced.
    fn pack(self, firmware_options: &FirmwareOptions) -> Result<()> {
        let (workspace_root, target_dir) = kernel_paths();

        info!("Building dynamic firmware for packing");
        let mut dynamic_spec = resolve(&BuildArgs::dynamic(
            firmware_options.debug,
            firmware_options.config_file.clone(),
        ))
        .context("failed to resolve dynamic firmware build inputs for packing")?;
        dynamic_spec.override_artifact_suffix("dynamic-pack");
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
            .join("firmware")
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

        let its_name = format!("{}.its", self.package_name());
        let itb_name = format!("{}.itb", self.package_name());
        let status = Command::new("mkimage")
            .args(["-f", &its_name, &itb_name])
            .current_dir(&target_dir)
            .status()
            .context("failed to execute mkimage command")?;

        fs::remove_file(&its_dest)
            .with_context(|| format!("failed to clean up ITS file: '{}'", its_dest.display()))?;

        if !status.success() {
            bail!(
                "mkimage failed to pack the {} kernel image (ITS '{}', output '{}')",
                self.command_name(),
                target_dir.join(&its_name).display(),
                target_dir.join(&itb_name).display()
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

/// Shared arguments of `cargo prototyper test` and `bench`.
/// Absent options resolve against [`Scheme`] in [`run`].
#[derive(Debug, Args, Clone)]
pub struct KernelArgs {
    /// Pack Prototyper and the kernel into a single ITB image
    #[arg(long)]
    pub pack: bool,

    /// Only build the kernel and firmware without running them in QEMU
    #[arg(long)]
    pub no_run: bool,

    /// Number of harts QEMU boots the kernel with (default: test 1, bench 4)
    #[arg(long)]
    pub smp: Option<usize>,

    /// Timeout in seconds of one QEMU attempt (default: test 60, bench 90)
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Number of QEMU attempts; retries happen only after a timeout
    /// (default: test 2, bench 4)
    #[arg(long)]
    pub retries: Option<usize>,

    /// Build the firmware in the debug profile instead of release
    #[arg(long)]
    pub debug: bool,

    /// Specify the path to a custom configuration file for the firmware
    #[arg(long, short = 'c')]
    pub config_file: Option<PathBuf>,
}

/// One QEMU run, with CLI overrides resolved against the Scheme.
#[derive(Debug, Clone, Copy)]
pub(super) struct ResolvedRun {
    pub(super) no_run: bool,
    pub(super) smp: usize,
    pub(super) timeout_secs: u64,
    pub(super) attempts: usize,
}

impl ResolvedRun {
    /// Resolves CLI overrides against the scheme defaults for one kernel.
    /// Pure: the single place `--smp`/`--timeout`/`--retries` turn into
    /// concrete values, so `None` handling is testable without spawning
    /// builds.
    pub(super) fn resolve(args: &KernelArgs, kernel: Kernel, scheme: &Scheme) -> Self {
        let defaults = scheme.action(kernel.into());
        ResolvedRun {
            no_run: args.no_run,
            smp: args.smp.unwrap_or(defaults.smp),
            timeout_secs: args.timeout.unwrap_or(defaults.timeout_secs),
            attempts: args.retries.unwrap_or(defaults.attempts),
        }
    }

    /// Rejects nonsensical values before anything is built, so `--no-run`
    /// invocations fail fast too. `qemu::run` repeats these checks as
    /// defense in depth.
    pub(super) fn validate(&self) -> Result<()> {
        if self.attempts == 0 {
            bail!("QEMU attempts must be at least 1 (got --retries 0)");
        }
        if self.smp == 0 {
            bail!("QEMU hart count must be at least 1 (got --smp 0)");
        }
        if self.timeout_secs == 0 {
            bail!("QEMU timeout must be at least 1 second (got --timeout 0)");
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
pub(super) fn run(kernel: Kernel, args: &KernelArgs) -> Result<ExitStatus> {
    let scheme = Scheme::default();
    let run_opts = ResolvedRun::resolve(args, kernel, &scheme);
    run_opts.validate()?;
    let firmware_options = FirmwareOptions {
        debug: args.debug,
        config_file: args.config_file.clone(),
    };

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

    if args.pack {
        kernel.pack(&firmware_options)?;
    }

    if !run_opts.no_run {
        let firmware_elf = spec
            .artifact_dir()
            .join(format!("{PACKAGE_NAME}-{}.elf", spec.artifact_suffix()));
        qemu::run(&QemuRun {
            bios: firmware_elf,
            qemu: scheme.qemu.clone(),
            smp: run_opts.smp,
            timeout: Duration::from_secs(run_opts.timeout_secs),
            attempts: run_opts.attempts,
            expected: kernel.expected_patterns(run_opts.smp)?,
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
        cargo_target_dir()
            .join(Target::Kernel.triple())
            .join("release"),
    )
}

/// Console substrings that mark a failed QEMU boot even when the process
/// exits successfully, read from `firmware/scripts/qemu-forbidden.txt` —
/// the single source shared with `.github/scripts/prototyper-qemu-boot.sh`.
pub(super) fn forbidden_patterns() -> Result<Vec<String>> {
    read_console_patterns(
        &workspace_root()
            .join("firmware")
            .join("scripts")
            .join("qemu-forbidden.txt"),
    )
}

/// Read console patterns from a pattern file: one fixed string per line,
/// skipping blank lines and `#` comments.
pub(super) fn read_console_patterns(path: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read console pattern file '{}'", path.display()))?;
    let patterns: Vec<String> = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect();
    // Fail closed: an empty pattern set verifies nothing.
    if patterns.is_empty() {
        bail!(
            "console pattern file '{}' contains no patterns",
            path.display()
        );
    }
    Ok(patterns)
}
