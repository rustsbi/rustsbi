//! Integration tests driving the compiled xtask binary.

use std::path::PathBuf;

use assert_cmd::Command;
use tempfile::TempDir;

fn xtask() -> Command {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
}

/// Both riscv targets the prototyper pipeline needs.
fn riscv_targets_installed() -> bool {
    let installed = std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default();
    installed.contains("riscv64gc-unknown-none-elf")
        && installed.contains("riscv64imac-unknown-none-elf")
}

#[test]
fn help_smoke_and_unknown_flag_fail() {
    xtask().arg("--help").assert().success();
    xtask()
        .arg("prototyper")
        .arg("test")
        .arg("--help")
        .assert()
        .success();
    xtask()
        .arg("prototyper")
        .arg("bench")
        .arg("--help")
        .assert()
        .success();
    xtask()
        .arg("prototyper")
        .arg("test")
        .arg("--definitely-not-a-flag")
        .assert()
        .failure();
}

#[test]
fn test_no_run_builds_payload_firmware() {
    if !riscv_targets_installed() {
        eprintln!("skipping: riscv targets not installed");
        return;
    }
    let target_dir = TempDir::new().unwrap();
    xtask()
        .args(["prototyper", "test", "--no-run", "--debug"])
        .env("CARGO_TARGET_DIR", target_dir.path())
        .assert()
        .success();
    let firmware: PathBuf = target_dir
        .path()
        .join("riscv64gc-unknown-none-elf/debug/rustsbi-prototyper-payload-test.bin");
    assert!(
        firmware.exists(),
        "missing artifact: {}",
        firmware.display()
    );
}
