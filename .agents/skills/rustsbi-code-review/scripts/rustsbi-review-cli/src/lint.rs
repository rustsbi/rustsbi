//! lint.rs — clippy and typos checks (advisory).
//!
//! Clippy findings don't block the review, but they appear in the report so
//! the LLM pass doesn't duplicate them.

use crate::build_stage::{RV64, RV64GC};
use crate::model::{CheckResult, Gate, ScopeInfo, Stage, Status};
use crate::util::{find_tool, first_line, must_run};
use std::time::Duration;

const HOST_ONLY: [&str; 1] = ["xtask"];

fn check_clippy(pkg: &str, target: Option<&str>) -> CheckResult {
    let mut args: Vec<&str> = vec!["cargo", "clippy", "--locked", "-p", pkg];
    if let Some(t) = target {
        args.push("--target");
        args.push(t);
    }
    let cmd_str = args.join(" ");
    let out = must_run("cargo", &args[1..], Duration::from_secs(600));

    let combined = format!("{}{}", out.stdout, out.stderr);
    let warnings: Vec<&str> = combined
        .lines()
        .filter(|l| l.starts_with("warning:") && !l.contains("generated"))
        .collect();

    let id = format!("clippy-{pkg}");
    let result = |status: Status, msg: String| {
        CheckResult::new(&id, Stage::Lint, status, Gate::Advisory, out.duration_ms, &cmd_str, &msg)
    };

    if !out.success && combined.to_lowercase().contains("error") {
        let output = if out.stderr.is_empty() { &out.stdout } else { &out.stderr };
        let status = if out.timed_out { Status::Timeout } else { Status::Error };
        return result(status, first_line(output)).with_output(output);
    }
    if !warnings.is_empty() {
        let head: Vec<&str> = warnings.iter().take(20).copied().collect();
        return result(Status::Fail, format!("{} warning(s)", warnings.len()))
            .with_output(&head.join("\n"));
    }
    result(Status::Pass, "ok".to_string())
}

fn check_typos() -> CheckResult {
    if find_tool("typos").is_none() {
        return CheckResult::new(
            "typos", Stage::Lint, Status::Skip, Gate::Advisory, 0, "", "typos not installed",
        );
    }
    let out = must_run("typos", &[], Duration::from_secs(120));
    if out.success {
        return CheckResult::new(
            "typos", Stage::Lint, Status::Pass, Gate::Advisory, out.duration_ms, "typos", "ok",
        );
    }
    let output = if out.stdout.is_empty() { &out.stderr } else { &out.stdout };
    CheckResult::new(
        "typos", Stage::Lint, Status::Fail, Gate::Advisory, out.duration_ms,
        "typos", &first_line(output),
    )
    .with_output(output)
}

pub fn run_lint(scope: &ScopeInfo) -> Vec<CheckResult> {
    if !scope.skipped_reason.is_empty() {
        return vec![CheckResult::new(
            "lint", Stage::Lint, Status::Skip, Gate::Advisory, 0, "", &scope.skipped_reason,
        )];
    }
    if scope.affected_packages.is_empty() {
        return vec![CheckResult::new(
            "lint", Stage::Lint, Status::Skip, Gate::Advisory, 0, "", "no affected packages",
        )];
    }

    let mut results = Vec::new();
    for pkg in &scope.affected_packages {
        let result = if HOST_ONLY.contains(&pkg.as_str()) {
            check_clippy(pkg, None)
        } else if pkg.as_str() == "rustsbi-prototyper" {
            check_clippy(pkg, Some(RV64GC))
        } else {
            check_clippy(pkg, Some(RV64))
        };
        results.push(result);
    }
    results.push(check_typos());
    results
}
