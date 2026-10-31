//! checks subcommand — the deterministic check pipeline.
//!
//! Stages: scope -> convention -> build -> lint -> test -> report.
//! Rust port of scripts/checks/cli.py.

use crate::model::{CheckResult, Gate, Stage, Status};

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
            "all", Stage::Scope, Status::Skip, Gate::Compile, 0, "",
            &scope.skipped_reason,
        )];
        let out = crate::report::write_report(output, &scope, &results)?;
        println!("{}: skipped ({})", out.json_path, scope.skipped_reason);
        return Ok(0);
    }

    println!(
        "scope: mode={} head={} packages={} dirty={}",
        scope.mode_str(), scope.head, scope.affected_packages.join(", "), scope.dirty
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
            "lint-test", Stage::Lint, Status::Skip, Gate::Advisory, 0, "",
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
        out.json_path, out.verdict.overall, out.verdict.review_allowed,
        out.md_path, total_ms as f64 / 1000.0, results.len()
    );

    Ok(if out.verdict.review_allowed { 0 } else { 1 })
}

fn print_stage(results: &[CheckResult]) {
    for r in results {
        println!("  {}: {} ({}ms)", r.id, r.status.as_str(), r.duration_ms);
    }
}
