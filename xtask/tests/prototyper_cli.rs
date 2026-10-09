//! Integration tests driving the compiled xtask binary.

use std::{
    fs,
    path::{Path, PathBuf},
};

use assert_cmd::Command;
use tempfile::TempDir;

fn xtask() -> Command {
    Command::new(env!("CARGO_BIN_EXE_xtask"))
}

/// Returns whether the RISC-V targets used by these CLI tests are installed.
fn riscv_targets_installed() -> bool {
    let installed = std::process::Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default();
    installed.contains("riscv64gc-unknown-none-elf")
        && installed.contains("riscv64imac-unknown-none-elf")
}

fn boot_stack_size(firmware: &Path) -> usize {
    let output = std::process::Command::new("rust-nm")
        .arg("--print-size")
        .arg(firmware)
        .output()
        .expect("rust-nm is required to inspect the firmware ELF");
    assert!(output.status.success(), "rust-nm failed: {output:?}");
    let symbols = String::from_utf8(output.stdout).unwrap();
    let address = |symbol: &str| {
        let line = symbols
            .lines()
            .find(|line| line.split_whitespace().last() == Some(symbol))
            .unwrap_or_else(|| panic!("firmware ELF has no {symbol} symbol"));
        usize::from_str_radix(line.split_whitespace().next().unwrap(), 16).unwrap()
    };
    address("sbi_boot_stack_end")
        .checked_sub(address("sbi_boot_stack_start"))
        .expect("firmware ELF has reversed bootstrap stack bounds")
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
fn configured_boot_topology_builds_and_rejects_invalid_bounds() {
    if !riscv_targets_installed() {
        eprintln!("skipping: riscv targets not installed");
        return;
    }
    let target_dir = TempDir::new().unwrap();
    let yuzuki_neko = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../firmware/prototyper/config/yuzuki-neko.toml");
    xtask()
        .args(["prototyper", "build", "--debug", "--config-file"])
        .arg(yuzuki_neko)
        .env("CARGO_TARGET_DIR", target_dir.path())
        .assert()
        .success();
    let single_hart_elf = target_dir
        .path()
        .join("riscv64gc-unknown-none-elf/debug/rustsbi-prototyper-dynamic.elf");
    assert_eq!(boot_stack_size(&single_hart_elf), 16 * 1024);
    assert!(
        target_dir
            .path()
            .join("prototyper/generated_config.rs")
            .exists()
    );

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
    let default_elf = target_dir
        .path()
        .join("riscv64gc-unknown-none-elf/debug/rustsbi-prototyper-payload-test.elf");
    assert_eq!(boot_stack_size(&default_elf), 16 * 1024);

    // An ELF-size check cannot catch debug discovery overflowing this stack.
    if std::process::Command::new("qemu-system-riscv64")
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
    {
        xtask()
            .args([
                "prototyper",
                "test",
                "--debug",
                "--smp",
                "4",
                "--timeout",
                "15",
                "--retries",
                "1",
            ])
            .env("CARGO_TARGET_DIR", target_dir.path())
            .assert()
            .success();
    } else {
        eprintln!("skipping debug boot: qemu-system-riscv64 not installed");
    }

    let default_config = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../firmware/prototyper/config/default.toml"),
    )
    .unwrap();
    let small_stack_config = target_dir.path().join("small-stack.toml");
    fs::write(
        &small_stack_config,
        format!("num_hart_max = 1\nstack_size_per_hart = 8192\n{default_config}"),
    )
    .unwrap();
    xtask()
        .args(["prototyper", "build", "--debug", "--config-file"])
        .arg(&small_stack_config)
        .env("CARGO_TARGET_DIR", target_dir.path())
        .assert()
        .success();
    assert_eq!(boot_stack_size(&single_hart_elf), 8192);

    let larger_topology_config = target_dir.path().join("larger-topology.toml");
    fs::write(
        &larger_topology_config,
        format!("num_hart_max = 16\n{default_config}"),
    )
    .unwrap();
    xtask()
        .args(["prototyper", "build", "--debug", "--config-file"])
        .arg(&larger_topology_config)
        .env("CARGO_TARGET_DIR", target_dir.path())
        .assert()
        .success();
    assert_eq!(boot_stack_size(&single_hart_elf), 16384);

    let invalid_config = target_dir.path().join("invalid.toml");
    for (key, value, error) in [
        (
            "stack_size_per_hart",
            128,
            "stack size must exceed the trap frame",
        ),
        (
            "stack_size_per_hart",
            8193,
            "stack size must be a multiple of the stack alignment",
        ),
    ] {
        fs::write(
            &invalid_config,
            format!("{key} = {value}\n{default_config}"),
        )
        .unwrap();
        let result = xtask()
            .args(["prototyper", "build", "--debug", "--config-file"])
            .arg(&invalid_config)
            .env("CARGO_TARGET_DIR", target_dir.path())
            .assert()
            .failure();
        assert!(
            String::from_utf8_lossy(&result.get_output().stderr).contains(error),
            "expected {error:?} in stderr: {}",
            String::from_utf8_lossy(&result.get_output().stderr)
        );
    }
}
