use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use clap::Args;

use crate::utils::cargo;

#[derive(Debug, Args, Clone)]
pub struct BenchArg {
    /// Package Prototyper and bench kernel into a single image
    #[clap(
        long,
        help = "Create a combined image with Prototyper and bench kernel"
    )]
    pub pack: bool,
}

const ARCH: &str = "riscv64imac-unknown-none-elf";
const BENCH_KERNEL_NAME: &str = "rustsbi-bench-kernel";
const PROTOTYPER_BIN: &str = "rustsbi-prototyper.bin";

#[must_use]
pub fn run(arg: &BenchArg) -> Option<ExitStatus> {
    let current_dir = env::current_dir().ok()?;
    let target_dir = get_target_dir(&current_dir);

    // Build the bench kernel
    info!("Building bench kernel");
    let build_status = build_bench_kernel()?;
    if !build_status.success() {
        error!("Failed to build bench kernel");
        return Some(build_status);
    }

    // Convert to binary format
    info!("Converting to binary format");
    let exit_status = convert_to_binary(&target_dir)?;
    if !exit_status.success() {
        error!("Failed to convert bench kernel to binary format");
        return Some(exit_status);
    }

    // Pack into image if requested
    if arg.pack {
        info!("Packing into image");
        match pack_image(&current_dir, &target_dir) {
            Ok(status) => {
                info!(
                    "Output image created at: {}",
                    target_dir
                        .join(format!("{}.itb", BENCH_KERNEL_NAME))
                        .display()
                );
                return Some(status);
            }
            Err(err_msg) => {
                error!("{}", err_msg);
                return Some(<ExitStatus as std::os::unix::process::ExitStatusExt>::from_raw(1));
            }
        }
    } else {
        info!(
            "Output binary created at: {}",
            target_dir
                .join(format!("{}.bin", BENCH_KERNEL_NAME))
                .display()
        );
    }

    Some(exit_status)
}

fn get_target_dir(current_dir: &Path) -> PathBuf {
    current_dir.join("target").join(ARCH).join("release")
}

fn build_bench_kernel() -> Option<ExitStatus> {
    cargo::Cargo::new("build")
        .package(BENCH_KERNEL_NAME)
        .target(ARCH)
        .release()
        .status()
        .ok()
}

fn convert_to_binary(target_dir: &Path) -> Option<ExitStatus> {
    let kernel_path = target_dir.join(BENCH_KERNEL_NAME);
    let bin_path = target_dir.join(format!("{}.bin", BENCH_KERNEL_NAME));

    Command::new("rust-objcopy")
        .args([
            "-O",
            "binary",
            "--binary-architecture=riscv64",
            &kernel_path.to_string_lossy(),
            &bin_path.to_string_lossy(),
        ])
        .status()
        .ok()
}

fn pack_image(current_dir: &Path, target_dir: &Path) -> Result<ExitStatus, String> {
    // Check if prototyper binary exists
    let prototyper_bin_path = target_dir.join(PROTOTYPER_BIN);
    if !prototyper_bin_path.exists() {
        return Err(format!(
            "Error: Prototyper binary not found at '{}'\n\
             Please run 'cargo prototyper' first to build the Prototyper binary.",
            prototyper_bin_path.display()
        ));
    }

    // Copy ITS file
    let its_source = current_dir
        .join("prototyper")
        .join("bench-kernel")
        .join("scripts")
        .join(format!("{}.its", BENCH_KERNEL_NAME));

    let its_dest = target_dir.join(format!("{}.its", BENCH_KERNEL_NAME));

    fs::copy(&its_source, &its_dest).map_err(|e| format!("Failed to copy ITS file: {}", e))?;

    // Change to target directory
    let original_dir =
        env::current_dir().map_err(|e| format!("Failed to get current directory: {}", e))?;

    env::set_current_dir(target_dir)
        .map_err(|e| format!("Failed to change directory to target: {}", e))?;

    // Create image
    let status = Command::new("mkimage")
        .args([
            "-f",
            &format!("{}.its", BENCH_KERNEL_NAME),
            &format!("{}.itb", BENCH_KERNEL_NAME),
        ])
        .status()
        .map_err(|e| format!("Failed to execute mkimage command: {}", e))?;

    // Clean up
    fs::remove_file(format!("{}.its", BENCH_KERNEL_NAME))
        .map_err(|e| format!("Failed to clean up ITS file: {}", e))?;

    // Restore original directory
    env::set_current_dir(original_dir)
        .map_err(|e| format!("Failed to restore original directory: {}", e))?;

    Ok(status)
}
