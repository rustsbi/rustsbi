// SPDX-License-Identifier: MulanPSL-2.0

#![cfg(target_os = "linux")]

use serde_json::Value;
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

#[test]
fn detached_helper_records_usage_at_the_runtime_skill_location() {
    let fixture = Fixture::new();
    let skill = fixture.root.join("relocated-skill");
    fs::create_dir(&skill).unwrap();
    let output = fixture.root.join("plan-relocated");
    let result = fixture
        .command(&format!("plan build \"{}\"", output.display()))
        .env_remove("RDB_USAGE_DIR")
        .env("RDB_SKILL_HOME", &skill)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(skill.join("usage/USAGE.log").is_file());
}

const TOOL: &str = r#"#!/bin/sh
name=$(basename "$0")
case "$name" in
rustc)
    printf 'rustc 1.88.0\nhost: x86_64-unknown-linux-gnu\n'
    ;;
rustup)
    case "$1" in
    component) printf 'rustfmt-x86_64-unknown-linux-gnu\nclippy-x86_64-unknown-linux-gnu\nrust-src\nllvm-tools-x86_64-unknown-linux-gnu\n' ;;
    target) printf 'riscv64gc-unknown-none-elf\nriscv64imac-unknown-none-elf\nriscv32imac-unknown-none-elf\n' ;;
    *) exit 91 ;;
    esac
    ;;
cargo)
    printf '%s\n' "$*" >> "$RDB_TEST_ROOT/calls"
    while [ -e "$RDB_TEST_ROOT/hold" ]; do sleep 0.05; done
    if [ -e "$RDB_TEST_ROOT/fail" ]; then printf 'compiler fixture failure\n' >&2; exit 37; fi
    if [ "$1 $2" = 'prototyper build' ]; then
        release=target/riscv64gc-unknown-none-elf/release
        mkdir -p "$release"
        printf ELF > "$release/rustsbi-prototyper-$3.elf"
        if [ ! -e "$RDB_TEST_ROOT/missing-bin" ]; then printf BIN > "$release/rustsbi-prototyper-$3.bin"; fi
    fi
    ;;
qemu-system-riscv64) printf 'QEMU emulator version 10.2.1\n' ;;
*) exit 0 ;;
esac
"#;

struct Fixture {
    root: PathBuf,
    repo: PathBuf,
    bin: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "rdb-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let repo = root.join("repo with spaces");
        for name in [
            "Cargo.toml",
            ".cargo/config.toml",
            "firmware/prototyper/Cargo.toml",
            "library/rustsbi/Cargo.toml",
            ".github/scripts/prototyper-qemu-boot.sh",
        ] {
            let path = repo.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "fixture").unwrap();
        }
        fs::write(
            repo.join("rust-toolchain.toml"),
            "[toolchain]\nchannel = 'test-toolchain'\n",
        )
        .unwrap();
        let bin = root.join("commands");
        fs::create_dir(&bin).unwrap();
        for name in [
            "cargo",
            "rustc",
            "rustup",
            "rust-objcopy",
            "cc",
            "qemu-system-riscv64",
        ] {
            fs::write(bin.join(name), TOOL).unwrap();
            fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o755)).unwrap();
        }
        for name in ["basename", "mkdir", "sleep"] {
            symlink(format!("/usr/bin/{name}"), bin.join(name)).unwrap();
        }
        Self { root, repo, bin }
    }

    fn command(&self, raw: &str) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_rustsbi-dev-build"));
        command
            .arg(raw)
            .current_dir(&self.repo)
            .env("PATH", &self.bin)
            .env("RDB_TEST_ROOT", &self.root)
            .env("RDB_USAGE_DIR", self.root.join("usage"));
        command
    }

    fn invoke(&self, mode: &str, suite: &str, output: &Path) -> Output {
        self.command(&format!(
            "{mode} {suite} --repo \"{}\" \"{}\"",
            self.repo.display(),
            output.display()
        ))
        .output()
        .unwrap()
    }

    fn report(&self, output: &Path) -> Value {
        serde_json::from_slice(&fs::read(output.join("report.json")).unwrap()).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn plan_preserves_quoted_paths_without_executing_commands() {
    let fixture = Fixture::new();
    let output = fixture.root.join("report $(touch sentinel)");
    let result = fixture.invoke("plan", "all", &output);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fixture.report(&output)["status"], "planned");
    assert!(!fixture.root.join("calls").exists());
    assert!(!fixture.repo.join("target").exists());
    assert!(!fixture.repo.join("sentinel").exists());
}

#[test]
fn output_collision_preserves_existing_data() {
    let fixture = Fixture::new();
    let output = fixture.root.join("report");
    fs::create_dir(&output).unwrap();
    fs::write(output.join("keep"), "user data").unwrap();
    assert_eq!(
        fixture.invoke("run", "build", &output).status.code(),
        Some(2)
    );
    assert_eq!(
        fs::read_to_string(output.join("keep")).unwrap(),
        "user data"
    );
    assert!(!fixture.root.join("calls").exists());
}

#[test]
fn symlink_output_is_refused() {
    let fixture = Fixture::new();
    let output = fixture.root.join("alias");
    symlink(&fixture.repo, &output).unwrap();
    assert_eq!(
        fixture.invoke("plan", "build", &output).status.code(),
        Some(2)
    );
    assert!(!fixture.repo.join("report.json").exists());
}

#[test]
fn command_failure_is_logged_and_stops_the_plan() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("fail"), "").unwrap();
    let output = fixture.root.join("failed");
    assert_eq!(
        fixture.invoke("run", "build", &output).status.code(),
        Some(1)
    );
    let report = fixture.report(&output);
    assert_eq!(report["status"], "failed");
    assert_eq!(report["steps"][0]["exit_code"], 37);
    assert_eq!(report["steps"][1]["status"], "not-run");
    assert!(
        fs::read_to_string(report["steps"][0]["log"].as_str().unwrap())
            .unwrap()
            .contains("compiler fixture failure")
    );
    assert_eq!(
        fs::read_to_string(fixture.root.join("calls"))
            .unwrap()
            .lines()
            .count(),
        1
    );
}

#[test]
fn missing_artifact_is_not_a_pass() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("missing-bin"), "").unwrap();
    let output = fixture.root.join("missing");
    assert_eq!(
        fixture.invoke("run", "build", &output).status.code(),
        Some(1)
    );
    assert!(
        fixture.report(&output)["error"]
            .as_str()
            .unwrap()
            .contains(".bin")
    );
}

#[test]
fn completed_build_is_not_boot_verification() {
    let fixture = Fixture::new();
    let output = fixture.root.join("success");
    let result = fixture.invoke("run", "build", &output);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report = fixture.report(&output);
    assert_eq!(report["status"], "passed");
    assert_eq!(report["boot_verified"], false);
    let usage: Value = serde_json::from_str(
        fs::read_to_string(fixture.root.join("usage/USAGE.log"))
            .unwrap()
            .trim(),
    )
    .unwrap();
    assert_eq!(usage["status"], "passed");
}

#[test]
fn missing_prerequisite_blocks_before_cargo() {
    let fixture = Fixture::new();
    fs::remove_file(fixture.bin.join("rust-objcopy")).unwrap();
    let output = fixture.root.join("blocked");
    assert_eq!(
        fixture.invoke("run", "build", &output).status.code(),
        Some(3)
    );
    assert_eq!(fixture.report(&output)["status"], "blocked");
    assert!(!fixture.root.join("calls").exists());
}

#[test]
fn invalid_explicit_qemu_does_not_fall_back_to_path() {
    let fixture = Fixture::new();
    let output = fixture.root.join("qemu");
    let raw = format!(
        "run build --qemu-bin \"{}/missing/qemu-system-riscv64\" \"{}\"",
        fixture.root.display(),
        output.display()
    );
    assert_eq!(
        fixture.command(&raw).output().unwrap().status.code(),
        Some(3)
    );
    assert!(
        fixture.report(&output)["error"]
            .as_str()
            .unwrap()
            .contains("selected QEMU")
    );
}

#[test]
fn overlapping_run_is_blocked_and_lock_is_released_after_exit() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("hold"), "").unwrap();
    let first = fixture.root.join("first");
    let mut child = fixture
        .command(&format!("run build \"{}\"", first.display()))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !fixture.root.join("calls").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert!(fixture.root.join("calls").exists());
    let second = fixture.root.join("second");
    assert_eq!(
        fixture.invoke("run", "build", &second).status.code(),
        Some(3)
    );
    fs::remove_file(fixture.root.join("hold")).unwrap();
    assert!(child.wait().unwrap().success());
    assert!(
        fixture
            .invoke("run", "build", &fixture.root.join("third"))
            .status
            .success()
    );
}

#[test]
fn absent_toolchain_channel_is_a_normal_input_error() {
    let fixture = Fixture::new();
    fs::write(fixture.repo.join("rust-toolchain.toml"), "[toolchain]\n").unwrap();
    let result = fixture.invoke("plan", "build", &fixture.root.join("bad"));
    assert_eq!(result.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&result.stderr).contains("panicked"));
}
