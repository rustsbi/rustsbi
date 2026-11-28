//! `cargo prototyper test` and `cargo prototyper bench` pipeline driven by `firmware/mbench`.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use clap::{Args, ValueEnum};

use crate::utils::{cargo, cargo_target_dir, objcopy, workspace_root};

use super::{
    Target,
    build::{BuildMode, build_firmware},
    config::{BuildSpec, FirmwareConfig, resolve_config},
    qemu::{self, QemuPayload, QemuRun, forbidden_patterns},
};

const MBENCH_PACKAGE: &str = "mbench-target";
const MBENCH_CLI_PACKAGE: &str = "mbench-cli";
const MBENCH_CLI_BIN: &str = "mbench";
const MBENCH_CARGO_DIR: &str = "mbench-cargo";
const PROTOTYPER_BIN: &str = "rustsbi-prototyper.bin";

fn mbench_manifest_path(workspace_root: &Path) -> Result<PathBuf> {
    let manifest = workspace_root
        .join("firmware")
        .join("mbench")
        .join("Cargo.toml");
    if !manifest.is_file() {
        bail!(
            "firmware/mbench submodule is not initialized (missing '{}'); \
             run `git submodule update --init --remote -- firmware/mbench`",
            manifest.display()
        );
    }
    Ok(manifest)
}

fn mbench_cargo_build(manifest: &Path, cargo_target_dir: &Path, package: &str) -> cargo::Cargo {
    let mut build = cargo::Cargo::new("build");
    build
        .manifest_path(manifest)
        .target_dir(cargo_target_dir)
        .package(package)
        .release()
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTFLAGS");
    build
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kernel {
    Test,
    Bench,
}

impl Kernel {
    pub(super) fn command_name(self) -> &'static str {
        match self {
            Kernel::Test => "test",
            Kernel::Bench => "bench",
        }
    }

    pub(super) const fn bootargs(self) -> &'static str {
        match self {
            Kernel::Test => "mode=check",
            Kernel::Bench => "mode=bench profile=quick",
        }
    }

    const fn default_timeout_secs(self) -> u64 {
        match self {
            Kernel::Test => 60,
            Kernel::Bench => 120,
        }
    }

    const fn default_attempts(self) -> usize {
        match self {
            Kernel::Test => 2,
            Kernel::Bench => 4,
        }
    }
}

/// Firmware delivery mode for `cargo prototyper test` / `bench`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
pub enum BootMode {
    /// Embed `mbench.bin` inside `rustsbi-prototyper` (`payload` feature).
    #[default]
    Payload,
    /// Load `mbench.elf` via QEMU `-kernel` into dynamic-mode firmware.
    Dynamic,
    /// Load `mbench.bin` at `jump_address` into jump-mode firmware.
    Jump,
}

/// Shared arguments of `cargo prototyper test` and `bench`.
#[derive(Debug, Args, Clone)]
pub struct KernelArgs {
    /// Pack dynamic Prototyper and mbench into a FIT ITB image (`<variant>/mbench.itb`)
    #[arg(long)]
    pub pack: bool,

    /// Only build mbench and firmware artifacts without running QEMU
    #[arg(long)]
    pub no_run: bool,

    /// How firmware loads mbench (`payload`, `dynamic`, or `jump`)
    #[arg(long, value_enum, default_value_t = BootMode::Payload)]
    pub mode: BootMode,

    /// QEMU machine model passed to `-machine` (e.g. `virt`, `virt,aclint=on`)
    #[arg(long, default_value = "virt")]
    pub machine: String,

    /// Actual runtime base address of payload-mode firmware when relocated by a prior loader (e.g. Miralis `0x80200000`)
    #[arg(long, value_parser = parse_address_arg)]
    pub firmware_load_addr: Option<u64>,

    /// Save captured QEMU serial output to this file
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Number of harts QEMU boots with (default: test 4, bench 4)
    #[arg(long)]
    pub smp: Option<usize>,

    /// Timeout in seconds of one QEMU attempt (default: test 60, bench 120)
    #[arg(long)]
    pub timeout: Option<u64>,

    /// Number of QEMU attempts; retries happen only after a timeout (default: test 2, bench 4)
    #[arg(long)]
    pub retries: Option<usize>,

    /// Build the firmware in the debug profile instead of release
    #[arg(long)]
    pub debug: bool,

    /// Specify the path to a custom configuration file for the firmware
    #[arg(long, short = 'c')]
    pub config_file: Option<PathBuf>,
}

/// Resolved options for one `test` or `bench` execution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResolvedRun {
    pub(super) pack: bool,
    pub(super) no_run: bool,
    pub(super) boot_mode: BootMode,
    pub(super) machine: String,
    pub(super) firmware_load_addr: Option<u64>,
    pub(super) smp: usize,
    pub(super) timeout_secs: u64,
    pub(super) attempts: usize,
}

impl ResolvedRun {
    pub(super) fn resolve(args: &KernelArgs, kernel: Kernel) -> Self {
        Self {
            pack: args.pack,
            no_run: args.no_run,
            boot_mode: args.mode,
            machine: args.machine.clone(),
            firmware_load_addr: args.firmware_load_addr,
            smp: args.smp.unwrap_or(4),
            timeout_secs: args
                .timeout
                .unwrap_or_else(|| kernel.default_timeout_secs()),
            attempts: args.retries.unwrap_or_else(|| kernel.default_attempts()),
        }
    }

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
        if self.machine.trim().is_empty() || self.machine.chars().any(char::is_whitespace) {
            bail!("--machine must be a non-empty QEMU machine string without whitespace");
        }
        if self.pack && self.boot_mode == BootMode::Jump {
            bail!("--pack cannot be combined with --mode jump");
        }
        if self.pack && self.firmware_load_addr.is_some() {
            bail!(
                "--pack packs dynamic firmware linked at `link_start_address` and cannot be \
                 combined with --firmware-load-addr"
            );
        }
        if let Some(addr) = self.firmware_load_addr {
            if self.boot_mode != BootMode::Payload {
                bail!("--firmware-load-addr is only valid with `--mode payload`");
            }
            if !self.no_run {
                bail!(
                    "--firmware-load-addr requires --no-run (it builds relocated payload firmware for an external loader)"
                );
            }
            if addr == 0 || !addr.is_multiple_of(0x1000) {
                bail!("--firmware-load-addr ({addr:#x}) must be non-zero and 0x1000-aligned");
            }
        }
        Ok(())
    }
}

fn parse_address_arg(raw: &str) -> std::result::Result<u64, String> {
    let trimmed = raw.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        u64::from_str_radix(hex, 16).map_err(|e| format!("invalid hex address {trimmed:?}: {e}"))
    } else {
        trimmed
            .parse::<u64>()
            .map_err(|e| format!("invalid integer address {trimmed:?}: {e}"))
    }
}

/// Derive the physical link address of `mbench-target` from firmware config before compilation.
fn derive_mbench_load_addr(
    config: &FirmwareConfig,
    boot_mode: BootMode,
    firmware_load_addr: Option<u64>,
) -> Result<u64> {
    let layout = config.layout;
    let load_addr = match boot_mode {
        BootMode::Payload => {
            let base = firmware_load_addr.unwrap_or(layout.link_start_address);
            let payload_offset = layout
                .payload_address
                .checked_sub(layout.link_start_address)
                .context("firmware payload_address precedes link_start_address")?;
            base.checked_add(payload_offset)
                .context("mbench load address overflow")?
        }
        BootMode::Jump => layout.jump_address,
        BootMode::Dynamic => layout.payload_address,
    };

    if load_addr == 0 || !load_addr.is_multiple_of(0x1000) {
        bail!("derived mbench load address ({load_addr:#x}) must be non-zero and 0x1000-aligned");
    }
    Ok(load_addr)
}

fn variant_slug(kernel: Kernel, load_addr: u64) -> String {
    format!("{}-{load_addr:#x}", kernel.command_name())
}

/// Render an explicit FIT image descriptor matching Prototyper's link address and mbench's load address.
fn render_fit_its(
    firmware_addr: u64,
    kernel_addr: u64,
    firmware_bin_name: &str,
    kernel_bin_name: &str,
) -> String {
    format!(
        r#"/dts-v1/;

/ {{
    description = "Configuration to load RustSBI before mbench";

    images {{
        kernel {{
            description = "mbench";
            data = /incbin/("./{kernel_bin_name}");
            type = "standalone";
            os = "u-boot";
            arch = "riscv";
            compression = "none";
            load = /bits/ 64 <{kernel_addr:#x}>;
        }};

        rustsbi {{
            description = "RustSBI Firmware";
            data = /incbin/("./{firmware_bin_name}");
            type = "firmware";
            os = "opensbi";
            arch = "riscv";
            compression = "none";
            load = /bits/ 64 <{firmware_addr:#x}>;
            entry = /bits/ 64 <{firmware_addr:#x}>;
        }};
    }};

    configurations {{
        default = "conf-1";

        conf-1 {{
            description = "RustSBI & mbench";
            firmware = "rustsbi";
            loadables = "kernel";
        }};
    }};
}};
"#
    )
}

struct MbenchArtifacts {
    elf: PathBuf,
    bin: PathBuf,
    variant_dir: PathBuf,
    load_addr: u64,
}

fn build_mbench_target(
    workspace_root: &Path,
    target_dir: &Path,
    kernel: Kernel,
    load_addr: u64,
) -> Result<MbenchArtifacts> {
    let manifest = mbench_manifest_path(workspace_root)?;
    let cargo_target_dir = target_dir.join(MBENCH_CARGO_DIR);
    let variant_dir = target_dir
        .join("mbench")
        .join(variant_slug(kernel, load_addr));
    fs::create_dir_all(&variant_dir).with_context(|| {
        format!(
            "failed to create mbench variant directory '{}'",
            variant_dir.display()
        )
    })?;

    let bootargs = kernel.bootargs();
    let formatted_addr = format!("{load_addr:#x}");

    info!(
        "Building {} mbench target (load_addr={}, bootargs={:?})",
        kernel.command_name(),
        formatted_addr,
        bootargs
    );
    let mut build = mbench_cargo_build(&manifest, &cargo_target_dir, MBENCH_PACKAGE);
    build
        .target(Target::Kernel.triple())
        .env("MBENCH_LOAD_ADDR", &formatted_addr)
        .env("MBENCH_DEFAULT_BOOTARGS", bootargs);

    let build_status = build.status().with_context(|| {
        format!(
            "failed to build package '{MBENCH_PACKAGE}' from '{}' for '{}'",
            manifest.display(),
            Target::Kernel.triple()
        )
    })?;
    if !build_status.success() {
        bail!(
            "failed to build {} mbench target; inspect cargo output above",
            kernel.command_name()
        );
    }

    let built_elf = cargo_target_dir
        .join(Target::Kernel.triple())
        .join("release")
        .join(MBENCH_PACKAGE);
    let elf = variant_dir.join("mbench.elf");
    let bin = variant_dir.join("mbench.bin");

    fs::copy(&built_elf, &elf).with_context(|| {
        format!(
            "failed to copy mbench ELF '{}' to '{}'",
            built_elf.display(),
            elf.display()
        )
    })?;

    info!("Converting mbench ELF to binary ({})", bin.display());
    let (objcopy_tool, objcopy_status) = objcopy::convert_elf_to_binary(&elf, &bin)?;
    if !objcopy_status.success() {
        bail!(
            "{objcopy_tool} failed to convert '{}' to binary",
            elf.display()
        );
    }

    Ok(MbenchArtifacts {
        elf,
        bin,
        variant_dir,
        load_addr,
    })
}

fn ensure_mbench_cli(workspace_root: &Path, target_dir: &Path) -> Result<PathBuf> {
    let manifest = mbench_manifest_path(workspace_root)?;
    let cargo_target_dir = target_dir.join(MBENCH_CARGO_DIR);

    info!("Building host mbench CLI ({MBENCH_CLI_PACKAGE})");
    let mut build = mbench_cargo_build(&manifest, &cargo_target_dir, MBENCH_CLI_PACKAGE);
    build.args(["--bin", MBENCH_CLI_BIN]);

    let build_status = build.status().with_context(|| {
        format!(
            "failed to build package '{MBENCH_CLI_PACKAGE}' from '{}'",
            manifest.display()
        )
    })?;
    if !build_status.success() {
        bail!("failed to build host `{MBENCH_CLI_PACKAGE}`; inspect cargo output above");
    }

    let cli_bin = cargo_target_dir
        .join("release")
        .join(format!("{MBENCH_CLI_BIN}{}", std::env::consts::EXE_SUFFIX));
    if !cli_bin.is_file() {
        bail!("expected mbench CLI executable at '{}'", cli_bin.display());
    }
    Ok(cli_bin)
}

fn pack_fit_image(
    artifacts: &MbenchArtifacts,
    dynamic_spec: Option<&BuildSpec>,
    firmware_config: &FirmwareConfig,
    debug: bool,
) -> Result<()> {
    let firmware_bin = if let Some(spec) = dynamic_spec {
        spec.artifact_path("bin")
    } else {
        info!("Building dynamic firmware for FIT packing");
        let pack_spec = BuildSpec::from_config(
            BuildMode::Dynamic,
            debug,
            firmware_config.clone(),
            Some("dynamic-pack".to_string()),
        );
        let build_status = build_firmware(&pack_spec)?;
        if !build_status.success() {
            bail!("failed to build dynamic firmware for FIT packing");
        }
        pack_spec.artifact_path("bin")
    };

    let staged_firmware = artifacts.variant_dir.join(PROTOTYPER_BIN);
    fs::copy(&firmware_bin, &staged_firmware).with_context(|| {
        format!(
            "failed to stage firmware binary '{}' -> '{}'",
            firmware_bin.display(),
            staged_firmware.display()
        )
    })?;

    let its_path = artifacts.variant_dir.join("mbench.its");
    let its_content = render_fit_its(
        firmware_config.layout.link_start_address,
        artifacts.load_addr,
        PROTOTYPER_BIN,
        "mbench.bin",
    );
    fs::write(&its_path, its_content)
        .with_context(|| format!("failed to write FIT descriptor '{}'", its_path.display()))?;

    let status = Command::new("mkimage")
        .args(["-f", "mbench.its", "mbench.itb"])
        .current_dir(&artifacts.variant_dir)
        .status()
        .context("failed to execute `mkimage` command")?;

    let _ = fs::remove_file(&its_path);
    let _ = fs::remove_file(&staged_firmware);

    if !status.success() {
        bail!(
            "mkimage failed to pack FIT image in '{}'",
            artifacts.variant_dir.display()
        );
    }

    info!(
        "FIT image created at: {}",
        artifacts.variant_dir.join("mbench.itb").display()
    );
    Ok(())
}

pub(super) fn run(kernel: Kernel, args: &KernelArgs) -> Result<()> {
    let run_opts = ResolvedRun::resolve(args, kernel);
    run_opts.validate()?;

    let workspace_root = workspace_root();
    let target_dir = cargo_target_dir();
    let firmware_config = resolve_config(args.config_file.as_deref())
        .context("failed to resolve prototyper firmware config")?;
    let load_addr = derive_mbench_load_addr(
        &firmware_config,
        run_opts.boot_mode,
        run_opts.firmware_load_addr,
    )?;

    let mbench_artifacts = build_mbench_target(&workspace_root, &target_dir, kernel, load_addr)?;

    let (firmware_spec, qemu_payload) = match run_opts.boot_mode {
        BootMode::Payload => (
            BuildSpec::from_config(
                BuildMode::Payload {
                    path: mbench_artifacts.bin.clone(),
                },
                args.debug,
                firmware_config.clone(),
                Some(format!("payload-{}", kernel.command_name())),
            ),
            QemuPayload::Payload,
        ),
        BootMode::Dynamic => (
            BuildSpec::from_config(
                BuildMode::Dynamic,
                args.debug,
                firmware_config.clone(),
                None,
            ),
            QemuPayload::Dynamic {
                kernel_elf: mbench_artifacts.elf.clone(),
                bootargs: kernel.bootargs(),
            },
        ),
        BootMode::Jump => (
            BuildSpec::from_config(BuildMode::Jump, args.debug, firmware_config.clone(), None),
            QemuPayload::Jump {
                kernel_bin: mbench_artifacts.bin.clone(),
                load_addr: mbench_artifacts.load_addr,
            },
        ),
    };

    let build_status = build_firmware(&firmware_spec)?;
    if !build_status.success() {
        bail!("failed to build prototyper firmware (exit {build_status})");
    }

    if args.pack {
        let dynamic_spec = (run_opts.boot_mode == BootMode::Dynamic).then_some(&firmware_spec);
        pack_fit_image(
            &mbench_artifacts,
            dynamic_spec,
            &firmware_config,
            args.debug,
        )?;
    }

    if !run_opts.no_run {
        let mbench_cli = ensure_mbench_cli(&workspace_root, &target_dir)?;
        qemu::run(&QemuRun {
            bios: firmware_spec.artifact_path("elf"),
            payload: qemu_payload,
            mbench_cli,
            work_dir: mbench_artifacts.variant_dir.clone(),
            kernel,
            machine: run_opts.machine,
            smp: run_opts.smp,
            timeout: Duration::from_secs(run_opts.timeout_secs),
            attempts: run_opts.attempts,
            forbidden: forbidden_patterns()?,
            out: args.out.clone(),
        })?;
    }

    Ok(())
}
