//! report.rs — aggregate results into checks.json and checks.md.
//!
//! review_allowed is false only when a compile-gated check failed.

use crate::model::{CheckResult, Gate, ScopeInfo, Status};
use crate::util::clip;
use anyhow::Result;
use serde_json::json;
use std::io::Write;

pub struct Verdict {
    pub review_allowed: bool,
    pub overall: &'static str,
    pub blocking_reason: String,
}

pub struct ReportOutcome {
    pub json_path: String,
    pub md_path: String,
    pub verdict: Verdict,
}

pub fn determine_verdict(results: &[CheckResult]) -> Verdict {
    let compile_fails: Vec<&CheckResult> = results.iter().filter(|r| r.is_compile_fail()).collect();
    if !compile_fails.is_empty() {
        let reasons = compile_fails
            .iter()
            .take(3)
            .map(|r| format!("{}: {}", r.id, r.message))
            .collect::<Vec<_>>()
            .join("; ");
        return Verdict {
            review_allowed: false,
            overall: "blocked",
            blocking_reason: format!("compile failure: {}", reasons),
        };
    }
    if results.iter().any(|r| r.status == Status::Fail) {
        return Verdict { review_allowed: true, overall: "failed", blocking_reason: String::new() };
    }
    Verdict { review_allowed: true, overall: "passed", blocking_reason: String::new() }
}

fn time_cell(ms: u64) -> String {
    if ms >= 100 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{}ms", ms)
    }
}

fn stage_str(stage: crate::model::Stage) -> &'static str {
    match stage {
        crate::model::Stage::Scope => "scope",
        crate::model::Stage::Convention => "convention",
        crate::model::Stage::Build => "build",
        crate::model::Stage::Lint => "lint",
        crate::model::Stage::Test => "test",
    }
}

fn gate_str(gate: Gate) -> &'static str {
    match gate {
        Gate::Compile => "compile",
        Gate::Advisory => "advisory",
    }
}

fn render_md(scope: &ScopeInfo, results: &[CheckResult], verdict: &Verdict, total_ms: u64) -> String {
    let mut l: Vec<String> = vec![
        "# Deterministic Checks Report".into(),
        String::new(),
        "## Summary".into(),
        String::new(),
        format!("- **overall**: `{}`", verdict.overall),
        format!("- **review_allowed**: `{}`", verdict.review_allowed),
    ];
    if !verdict.blocking_reason.is_empty() {
        l.push(format!("- **blocking**: {}", verdict.blocking_reason));
    }
    l.push(format!("- **fingerprint**: `{}`", scope.fingerprint));
    l.push(format!("- **mode**: `{}`", scope.mode_str()));
    if !scope.head.is_empty() {
        l.push(format!("- **head**: `{}`", scope.head));
    }
    if !scope.affected_packages.is_empty() {
        l.push(format!("- **packages**: `{}`", scope.affected_packages.join(", ")));
    }
    l.push(format!("- **total_time**: {:.1}s", total_ms as f64 / 1000.0));
    l.push(String::new());

    l.push("## Results".into());
    l.push(String::new());
    l.push("| stage | check | status | gate | time | message |".into());
    l.push("|-------|-------|--------|------|------|---------|".into());
    for r in results {
        let msg: String = r.message.replace('|', "/").chars().take(80).collect();
        l.push(format!(
            "| {} | `{}` | {} | {} | {} | {} |",
            stage_str(r.stage),
            r.id,
            r.status.icon(),
            gate_str(r.gate),
            time_cell(r.duration_ms),
            msg,
        ));
    }
    l.push(String::new());

    let failures: Vec<&CheckResult> = results
        .iter()
        .filter(|r| matches!(r.status, Status::Fail | Status::Error | Status::Timeout))
        .collect();
    if !failures.is_empty() {
        l.push("## Details".into());
        l.push(String::new());
        for r in failures {
            l.push(format!("### `{}` ({})", r.id, r.status.as_str()));
            l.push(String::new());
            l.push(format!("- command: `{}`", r.command));
            l.push(format!("- message: {}", r.message));
            l.push(String::new());
            if !r.output.is_empty() {
                l.push("```".into());
                l.push(clip(&r.output, 2048).unwrap_or(&r.output).to_string());
                l.push("```".into());
                l.push(String::new());
            }
        }
    }
    l.join("\n")
}

/// Write <output>.checks.json and <output>.checks.md.
pub fn write_report(output_path: &str, scope: &ScopeInfo, results: &[CheckResult]) -> Result<ReportOutcome> {
    let verdict = determine_verdict(results);
    let total_ms: u64 = results.iter().map(|r| r.duration_ms).sum();

    let data = json!({
        "schema": 1,
        "timestamp": chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        "scope": {
            "mode": scope.mode_str(),
            "base": scope.base,
            "head": scope.head,
            "branch": scope.branch,
            "fingerprint": scope.fingerprint,
            "affected_packages": scope.affected_packages,
            "dirty": scope.dirty,
            "all_packages": scope.all_packages,
        },
        "review_allowed": verdict.review_allowed,
        "overall": verdict.overall,
        "blocking_reason": verdict.blocking_reason,
        "results": results,
        "total_duration_ms": total_ms,
    });

    let json_path = format!("{}.checks.json", output_path);
    let mut f = std::fs::File::create(&json_path)?;
    serde_json::to_writer_pretty(&mut f, &data)?;
    f.write_all(b"\n")?;

    let md_path = format!("{}.checks.md", output_path);
    let md = render_md(scope, results, &verdict, total_ms);
    std::fs::write(&md_path, md + "\n")?;

    Ok(ReportOutcome { json_path, md_path, verdict })
}