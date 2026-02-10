use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use crate::utils::{CmdOptional, cargo};
use clap::Args;

#[derive(Debug, Args, Clone)]
pub struct ArcebootArg {
    /// Extra features for arceboot (comma or space separated)
    #[clap(long, short = 'f')]
    pub features: Vec<String>,

    /// Log level: warn, error, info, debug, trace
    #[clap(long, default_value = "debug")]
    pub log: String,

    /// Build in debug mode (default is release)
    #[clap(long)]
    pub debug: bool,

    /// Also build rustsbi-prototyper with arceboot as payload
    #[clap(long)]
    pub payload: bool,

    /// Target platform (default: riscv64-qemu-virt)
    #[clap(long, default_value = "riscv64-qemu-virt")]
    pub platform: String,

    /// Number of CPUs
    #[clap(long, default_value = "1")]
    pub smp: usize,

    /// Run in QEMU after building
    #[clap(long)]
    pub qemu: bool,

    /// Enable QEMU display (virtio-gpu), default is serial-only (nographic)
    #[clap(long)]
    pub display: bool,

    /// Path to the virtual disk image for QEMU
    #[clap(long, default_value = "arceboot/disk.img")]
    pub disk: String,
}

const ARCH: &str = "riscv64gc-unknown-none-elf";
const PACKAGE_NAME: &str = "arceboot";

#[must_use]
pub fn run(arg: &ArcebootArg) -> Option<ExitStatus> {
    let current_dir = env::current_dir().ok()?;
    let arceboot_dir = current_dir.join("arceboot");
    let target_dir = if arg.debug {
        current_dir.join("target").join(ARCH).join("debug")
    } else {
        current_dir.join("target").join(ARCH).join("release")
    };

    // Step 1: Generate config
    info!("Generating ArceBoot config for platform '{}'", arg.platform);
    let config_path = generate_config(&arceboot_dir, arg)?;
    info!("Config generated at: {}", config_path.display());

    // Step 2: Build arceboot
    info!("Building ArceBoot");
    let exit_status = build_arceboot(&arceboot_dir, &config_path, arg)?;
    if !exit_status.success() {
        error!("Failed to build ArceBoot");
        return Some(exit_status);
    }

    // Step 3: Convert ELF to binary
    info!("Converting ELF to binary with rust-objcopy");
    let exit_status = convert_to_binary(&target_dir)?;
    if !exit_status.success() {
        error!("Failed to convert ArceBoot ELF to binary format");
        return Some(exit_status);
    }

    let bin_path = target_dir.join(format!("{}.bin", PACKAGE_NAME));
    info!("ArceBoot binary created at: {}", bin_path.display());

    // Step 4: Optionally build prototyper with arceboot as payload
    let sbi_path = if arg.payload || arg.qemu {
        info!("Building RustSBI Prototyper with ArceBoot as payload");
        let prototyper_arg = crate::prototyper::PrototyperArg {
            features: Vec::new(),
            fdt: None,
            payload: Some(bin_path.to_string_lossy().to_string()),
            jump: false,
            debug: arg.debug,
            config_file: None,
            target: None,
        };
        let exit_status = crate::prototyper::run(&prototyper_arg)?;
        if !exit_status.success() {
            error!("Failed to build RustSBI Prototyper");
            return Some(exit_status);
        }
        Some(target_dir.join("rustsbi-prototyper-payload.elf"))
    } else {
        None
    };

    // Step 5: Optionally run QEMU
    if arg.qemu {
        let sbi = sbi_path.expect("SBI path should be available when --qemu is used");
        info!("Launching QEMU");
        return run_qemu(&sbi, arg);
    }

    Some(ExitStatus::default())
}

/// Run `axconfig-gen` to generate the config file from platform toml.
fn generate_config(arceboot_dir: &Path, arg: &ArcebootArg) -> Option<PathBuf> {
    // Ensure axconfig-gen is installed
    ensure_axconfig_gen();

    let defconfig = arceboot_dir.join("configs").join("defconfig.toml");
    let plat_config = arceboot_dir
        .join("configs")
        .join("platforms")
        .join(format!("{}.toml", arg.platform));

    if !plat_config.exists() {
        // List available platforms
        let platforms_dir = arceboot_dir.join("configs").join("platforms");
        let available: Vec<String> = fs::read_dir(&platforms_dir)
            .ok()?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                e.path()
                    .file_stem()
                    .and_then(|s| s.to_str().map(String::from))
            })
            .collect();
        error!(
            "Platform '{}' not found. Available platforms: {}",
            arg.platform,
            available.join(", ")
        );
        return None;
    }

    let out_config = arceboot_dir.join(".axconfig.toml");
    let arch = arg.platform.split('-').next().unwrap_or("riscv64");

    let status = Command::new("axconfig-gen")
        .arg(defconfig.to_string_lossy().as_ref())
        .arg(plat_config.to_string_lossy().as_ref())
        .args(["-w", &format!("smp={}", arg.smp)])
        .args(["-w", &format!("arch=\"{}\"", arch)])
        .args(["-w", &format!("platform=\"{}\"", arg.platform)])
        .args(["-o", out_config.to_string_lossy().as_ref()])
        .status()
        .ok()?;

    if !status.success() {
        error!("Failed to generate config with axconfig-gen");
        return None;
    }

    Some(out_config)
}

/// Ensure axconfig-gen is installed.
fn ensure_axconfig_gen() {
    if Command::new("axconfig-gen")
        .arg("--version")
        .output()
        .is_err()
    {
        info!("Installing axconfig-gen...");
        let _ = Command::new(env!("CARGO"))
            .args(["install", "axconfig-gen"])
            .status();
    }
}

/// Build arceboot with proper config and linker script.
fn build_arceboot(
    arceboot_dir: &Path,
    config_path: &Path,
    arg: &ArcebootArg,
) -> Option<ExitStatus> {
    let linker_script = arceboot_dir.join("link.ld");
    let rustflags = format!(
        "-C opt-level=z -C panic=abort -C relocation-model=static -C target-cpu=generic -C link-arg=-T{}",
        linker_script.display()
    );

    cargo::Cargo::new("build")
        .package(PACKAGE_NAME)
        .target(ARCH)
        .env("RUSTFLAGS", &rustflags)
        .env("AX_CONFIG_PATH", config_path.to_string_lossy().as_ref())
        .env("AX_LOG", &arg.log)
        .optional(!arg.features.is_empty(), |cargo| {
            cargo.features(&arg.features)
        })
        .optional(!arg.debug, |cargo| cargo.release())
        .status()
        .ok()
}

/// Convert ELF to raw binary with rust-objcopy.
fn convert_to_binary(target_dir: &Path) -> Option<ExitStatus> {
    let elf_path = target_dir.join(PACKAGE_NAME);
    let bin_path = target_dir.join(format!("{}.bin", PACKAGE_NAME));

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
            Please install cargo-binutils: cargo install cargo-binutils"
        );
    }

    result
}

/// Launch QEMU with the built SBI firmware.
fn run_qemu(sbi_path: &Path, arg: &ArcebootArg) -> Option<ExitStatus> {
    let mut cmd = Command::new("qemu-system-riscv64");
    cmd.args(["-m", "128M", "-serial", "mon:stdio"]);
    cmd.args(["-bios", &sbi_path.to_string_lossy()]);
    cmd.args(["-machine", "virt"]);
    cmd.args([
        "-device",
        "virtio-blk-pci,drive=disk0",
        "-drive",
        &format!("id=disk0,if=none,format=raw,file={}", arg.disk),
    ]);

    if arg.display {
        cmd.args(["-device", "virtio-gpu-pci,xres=1024,yres=768"]);
    } else {
        cmd.arg("-nographic");
    }

    cmd.status().ok()
}
