//! checks subcommand — the deterministic check pipeline.
//!
//! Stages: scope -> convention -> build -> lint -> test -> report.

use crate::model::{CheckResult, Gate, Stage, Status};
use anyhow::{Context, Result};

/// Require an explicit gate verdict before starting a model review.
pub fn read_for_review(path: &str) -> Result<String> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("read checks report {path}"))?;
    let report: serde_json::Value =
        serde_json::from_str(&text).context("invalid checks report JSON")?;
    anyhow::ensure!(report["schema"] == 1, "unsupported checks report schema");
    match report["review_allowed"].as_bool() {
        Some(true) => Ok(text),
        Some(false) => anyhow::bail!(
            "review blocked by checks: {}",
            report["blocking_reason"]
                .as_str()
                .unwrap_or("compile gate did not pass")
        ),
        None => anyhow::bail!("checks report has no review_allowed verdict"),
    }
}

pub fn checks_main(argv: &[String]) -> i32 {
    if argv.len() < 2 {
        crate::util::die("usage: rustsbi-review-cli checks '<raw args>' <output>");
    }
    run(&argv[0], &argv[1]).unwrap_or_else(|e| {
        eprintln!("error: {e:#}");
        2
    })
}

fn run(raw_args: &str, output: &str) -> anyhow::Result<i32> {
    // Stage 1: scope.
    let scope = crate::scope::run_scope(raw_args);
    if !scope.skipped_reason.is_empty() {
        // Backtest mode (pr/N): skip all checks.
        let results = vec![CheckResult::new(
            "all",
            Stage::Scope,
            Status::Skip,
            Gate::Compile,
            0,
            "",
            &scope.skipped_reason,
        )];
        let out = crate::report::write_report(output, &scope, &results)?;
        println!("{}: skipped ({})", out.json_path, scope.skipped_reason);
        return Ok(0);
    }

    println!(
        "scope: mode={} head={} packages={} dirty={}",
        scope.mode_str(),
        scope.head,
        scope.affected_packages.join(", "),
        scope.dirty
    );

    let mut results: Vec<CheckResult> = Vec::new();

    // Stage 2: convention.
    println!("convention: running...");
    let convention = crate::convention::run_convention(&scope);
    print_stage(&convention);
    results.extend(convention);

    // Stage 3: build.
    println!("build: running...");
    let build = crate::build_stage::run_build(&scope);
    print_stage(&build);
    let compile_failed = build.iter().any(|r| r.is_compile_fail());
    results.extend(build);

    // A compile failure short-circuits lint and test.
    if compile_failed {
        println!("build: compile failure, skipping lint and test");
        results.push(CheckResult::new(
            "lint-test",
            Stage::Lint,
            Status::Skip,
            Gate::Advisory,
            0,
            "",
            "skipped: compile failure",
        ));
    } else {
        // Stage 4: lint.
        println!("lint: running...");
        let lint = crate::lint::run_lint(&scope);
        print_stage(&lint);
        results.extend(lint);

        // Stage 5: test.
        println!("test: running...");
        let test = crate::test_stage::run_test(&scope);
        print_stage(&test);
        results.extend(test);
    }

    // Stage 6: report.
    let out = crate::report::write_report(output, &scope, &results)?;
    let total_ms: u64 = results.iter().map(|r| r.duration_ms).sum();
    println!(
        "checks: {} overall={} review_allowed={}\nreport: {}\ntotal: {:.1}s, {} check(s)",
        out.json_path,
        out.verdict.overall,
        out.verdict.review_allowed,
        out.md_path,
        total_ms as f64 / 1000.0,
        results.len()
    );

    Ok(if out.verdict.overall == "error" {
        2
    } else if out.verdict.review_allowed {
        0
    } else {
        1
    })
}

fn print_stage(results: &[CheckResult]) {
    for r in results {
        println!("  {}: {} ({}ms)", r.id, r.status.as_str(), r.duration_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocked_or_incomplete_reports_cannot_start_review() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("checks.json");
        let path = path.to_str().unwrap();
        assert!(read_for_review(path).is_err());
        for report in [
            r#"{"schema":1,"review_allowed":false,"blocking_reason":"compile failure"}"#,
            r#"{"schema":1}"#,
            r#"{"schema":2,"review_allowed":true}"#,
            "not JSON",
        ] {
            std::fs::write(path, report)?;
            assert!(read_for_review(path).is_err(), "accepted {report}");
        }
        Ok(())
    }

    #[test]
    fn advisory_failures_remain_available_to_review() -> Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        let report = r#"{"schema":1,"review_allowed":true,"overall":"failed","results":[{"id":"test-xtask","status":"fail","gate":"advisory"}]}"#;
        std::fs::write(file.path(), report)?;
        assert_eq!(read_for_review(file.path().to_str().unwrap())?, report);
        Ok(())
    }
}
