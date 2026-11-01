//! build_stage.rs — cargo check/build matrix for affected packages.
//!
//! Compile failure blocks the LLM review (gate = Compile). Missing tools or
//! targets are reported as error/skip, not fail.

use crate::model::{CheckResult, Gate, ScopeInfo, Stage, Status};
use crate::util::{cargo_bin_env, die, find_tool, first_line, run_cmd};
use std::time::Duration;

pub const RV64: &str = "riscv64imac-unknown-none-elf";
pub const RV64GC: &str = "riscv64gc-unknown-none-elf";
pub const RV32: &str = "riscv32imac-unknown-none-elf";

/// Packages that don't support RV32 (from Library.yml).
const NO_RV32: [&str; 2] = ["pmpm", "smm"];

/// Feature matrix for rustsbi (from Library.yml).
const RUSTSBI_FEATURES: [&str; 4] = ["", "machine", "forward", "machine,forward"];

/// Firmware packages that need cargo-binutils for the prototyper build.
const FIRMWARE_PACKAGES: [&str; 3] = [
    "rustsbi-prototyper",
    "rustsbi-test-kernel",
    "rustsbi-bench-kernel",
];

/// Packages that only build on the host (not for RISC-V targets).
const HOST_ONLY: [&str; 1] = ["xtask"];

/// stderr fragments meaning "environment problem", not a code defect.
const ENV_ERROR_HINTS: [&str; 6] = [
    "could not find cargo",
    "toolchain",
    "target not installed",
    "no such file",
    "permission denied",
    "rust-objcopy",
];

/// Check id: "<verb>-<pkg>[-<target-arch>][-<features>]", e.g.
/// check-rustsbi-riscv64imac, build-rustsbi-riscv32imac-machine+forward.
fn make_id(verb: &str, pkg: &str, target: Option<&str>, features: &str) -> String {
    let mut parts = vec![verb.to_string(), pkg.to_string()];
    if let Some(t) = target {
        parts.push(t.split('-').next().unwrap_or(t).to_string());
    }
    if !features.is_empty() {
        parts.push(features.replace(',', "+"));
    }
    parts.join("-")
}

/// One `cargo <verb> --locked -p <pkg> [--target T] [--features F]` run.
fn run_cargo(verb: &str, pkg: &str, target: Option<&str>, features: &str, gate: Gate) -> CheckResult {
    let mut args: Vec<String> = vec![
        "cargo".into(),
        verb.into(),
        "--locked".into(),
        "-p".into(),
        pkg.into(),
    ];
    if let Some(t) = target {
        args.push("--target".into());
        args.push(t.into());
    }
    if !features.is_empty() {
        args.push("--features".into());
        args.push(features.into());
    }
    let cmd_str = args.join(" ");
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();

    let out = crate::util::must_run(refs[0], &refs[1..], Duration::from_secs(600));
    let id = make_id(verb, pkg, target, features);
    if out.success {
        return CheckResult::new(&id, Stage::Build, Status::Pass, gate, out.duration_ms, &cmd_str, "ok");
    }
    let is_env = ENV_ERROR_HINTS.iter().any(|h| out.stderr.to_lowercase().contains(*h));
    let status = match out.status() {
        Status::Fail if is_env => Status::Error,
        s => s,
    };
    let output = if out.stderr.is_empty() { &out.stdout } else { &out.stderr };
    CheckResult::new(&id, Stage::Build, status, gate, out.duration_ms, &cmd_str, &first_line(output))
        .with_output(output)
}

pub fn run_build(scope: &ScopeInfo) -> Vec<CheckResult> {
    if !scope.skipped_reason.is_empty() {
        return vec![CheckResult::new(
            "build", Stage::Build, Status::Skip, Gate::Compile, 0, "", &scope.skipped_reason,
        )];
    }
    if scope.affected_packages.is_empty() {
        // Docs-only or metadata-only change.
        return vec![CheckResult::new(
            "build", Stage::Build, Status::Skip, Gate::Compile, 0, "",
            "no affected packages (docs/metadata only)",
        )];
    }

    let mut results = Vec::new();
    let is_fw = |p: &str| FIRMWARE_PACKAGES.contains(&p);
    let is_host = |p: &str| HOST_ONLY.contains(&p);

    // Host-only packages (xtask): just check on the host.
    for pkg in &scope.affected_packages {
        if is_host(pkg) {
            results.push(run_cargo("check", pkg, None, "", Gate::Compile));
        }
    }

    // Library packages: cargo check on host + rv64 (+ rv32 if supported).
    for pkg in &scope.affected_packages {
        if is_fw(pkg) || is_host(pkg) {
            continue;
        }
        results.push(run_cargo("check", pkg, None, "", Gate::Compile));
        results.push(run_cargo("check", pkg, Some(RV64), "", Gate::Compile));
        if !NO_RV32.contains(&pkg.as_str()) {
            results.push(run_cargo("check", pkg, Some(RV32), "", Gate::Compile));
        }
    }

    // rustsbi: feature matrix builds.
    if scope.affected_packages.iter().any(|p| p == "rustsbi") {
        for feats in RUSTSBI_FEATURES {
            results.push(run_cargo("build", "rustsbi", Some(RV64), feats, Gate::Compile));
            results.push(run_cargo("build", "rustsbi", Some(RV32), feats, Gate::Compile));
        }
    }

    // sbi-testing: log feature build.
    if scope.affected_packages.iter().any(|p| p == "sbi-testing") {
        results.push(run_cargo("build", "sbi-testing", Some(RV64), "log", Gate::Compile));
    }

    // Firmware packages: need cargo-binutils.
    let fw: Vec<&String> = scope
        .affected_packages
        .iter()
        .filter(|p| is_fw(p))
        .collect();
    if !fw.is_empty() {
        if find_tool("rust-objcopy").is_none() {
            results.push(CheckResult::new(
                "build-firmware", Stage::Build, Status::Skip, Gate::Compile, 0, "",
                "cargo-binutils not installed; skipping firmware build",
            ));
        } else {
            // The prototyper generates its config before firmware builds.
            if fw.iter().any(|p| p.as_str() == "rustsbi-prototyper") {
                let out = run_cmd(
                    "cargo", &["prototyper", "build"],
                    Duration::from_secs(900),
                    &cargo_bin_env(),
                )
                .unwrap_or_else(|e| die(&format!("error: {e}")));
                let status = if out.success { Status::Pass } else { Status::Fail };
                let output = if out.stderr.is_empty() { &out.stdout } else { &out.stderr };
                let message = if out.success { "ok".to_string() } else { first_line(output) };
                results.push(
                    CheckResult::new(
                        "prototyper-build", Stage::Build, status, Gate::Compile,
                        out.duration_ms, "cargo prototyper build", &message,
                    )
                    .with_output(output),
                );
            }
            for pkg in fw {
                let target = if pkg.as_str() == "rustsbi-prototyper" { RV64GC } else { RV64 };
                results.push(run_cargo("check", pkg, Some(target), "", Gate::Compile));
            }
        }
    }

    results
}

