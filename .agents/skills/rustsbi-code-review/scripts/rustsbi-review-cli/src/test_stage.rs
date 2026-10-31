//! test_stage.rs — cargo test for affected packages.
//!
//! Test failures are reported but do not block the LLM review: a failing test
//! is a finding, not a reason to skip semantic review.

use crate::model::{CheckResult, Gate, ScopeInfo, Stage, Status};
use crate::util::{first_line, must_run};
use std::time::Duration;

/// rustsbi tests run both with and without the machine feature (from Library.yml).
const RUSTSBI_TEST_FEATURES: [&str; 2] = ["", "machine"];

fn check_test(pkg: &str, features: &str, lib_only: bool) -> CheckResult {
    let mut args: Vec<&str> = vec!["cargo", "test", "--locked", "-p", pkg];
    if lib_only {
        args.push("--lib");
    }
    if !features.is_empty() {
        args.push("--features");
        args.push(features);
    }
    let cmd_str = args.join(" ");
    let out = must_run(args[0], &args[1..], Duration::from_secs(600));

    let id = format!("test-{pkg}{}", if features.is_empty() { "" } else { "-features" });
    if out.success {
        let passed = out.stdout.lines().filter(|l| l.contains("test result: ok")).count();
        return CheckResult::new(
            &id, Stage::Test, Status::Pass, Gate::Advisory, out.duration_ms, &cmd_str,
            &format!("{passed} test suite(s) passed"),
        );
    }
    let output = if out.stderr.is_empty() { &out.stdout } else { &out.stderr };
    CheckResult::new(
        &id, Stage::Test, out.status(), Gate::Advisory, out.duration_ms, &cmd_str,
        &first_line(output),
    )
    .with_output(output)
}

pub fn run_test(scope: &ScopeInfo) -> Vec<CheckResult> {
    if !scope.skipped_reason.is_empty() {
        return vec![CheckResult::new(
            "test", Stage::Test, Status::Skip, Gate::Advisory, 0, "", &scope.skipped_reason,
        )];
    }
    if scope.affected_packages.is_empty() {
        return vec![CheckResult::new(
            "test", Stage::Test, Status::Skip, Gate::Advisory, 0, "", "no affected packages",
        )];
    }

    let mut results = Vec::new();
    for pkg in &scope.affected_packages {
        match pkg.as_str() {
            // CI convention: --lib only (firmware doc tests need RISC-V).
            "rustsbi" => {
                for feats in RUSTSBI_TEST_FEATURES {
                    results.push(check_test("rustsbi", feats, false));
                }
            }
            "rustsbi-firmware-runtime" => results.push(check_test(pkg, "", true)),
            // Prototyper bin has test=false; only macros are tested.
            "rustsbi-prototyper" => results.push(check_test("rustsbi-runtime-macros", "", false)),
            _ => results.push(check_test(pkg, "", false)),
        }
    }
    results
}
