//! convention.rs — format, whitespace, DCO, and changelog checks.
//!
//! Non-compiling checks that verify the change follows RustSBI's coding and
//! commit conventions. All advisory (reported, never blocking).

use crate::gitutil::{git_ok, must_git};
use crate::model::{CheckResult, Gate, Mode, ScopeInfo, Stage, Status};
use crate::util::{first_line, must_run};
use std::time::{Duration, Instant};

/// Library directories that require a CHANGELOG.md update (from Changelog.yml).
const CHANGELOG_DIRS: [&str; 5] = [
    "library/sbi-rt",
    "library/sbi-spec",
    "library/sbi-testing",
    "library/rustsbi",
    "library/macros",
];

fn not_applicable(id: &str, why: &str) -> CheckResult {
    CheckResult::new(id, Stage::Convention, Status::Skip, Gate::Advisory, 0, "", why)
}

/// cargo fmt --all -- --check
fn check_fmt() -> CheckResult {
    let out = must_run("cargo", &["fmt", "--all", "--", "--check"], Duration::from_secs(120));
    if out.success {
        return CheckResult::new(
            "fmt", Stage::Convention, Status::Pass, Gate::Advisory, out.duration_ms,
            "cargo fmt --all -- --check", "ok",
        );
    }
    let diffs = out.stdout.lines().filter(|l| l.starts_with("Diff in ")).count();
    let output = if out.stdout.is_empty() { &out.stderr } else { &out.stdout };
    let msg = if diffs > 0 {
        format!("{diffs} file(s) need formatting")
    } else {
        first_line(output)
    };
    CheckResult::new(
        "fmt", Stage::Convention, Status::Fail, Gate::Advisory, out.duration_ms,
        "cargo fmt --all -- --check", &msg,
    )
    .with_output(output)
}

/// git diff --check with autocrlf to handle Windows CRLF.
fn check_whitespace(scope: &ScopeInfo) -> CheckResult {
    if scope.mode != Mode::Diff || scope.base.is_empty() {
        return not_applicable("whitespace", "not applicable (files mode)");
    }
    let mb = git_ok(&["merge-base", scope.base.as_str(), "HEAD"]);
    let out = must_git(&["diff", "--check", &format!("{mb}..HEAD")]);
    if out.success {
        return CheckResult::new(
            "whitespace", Stage::Convention, Status::Pass, Gate::Advisory, out.duration_ms,
            "git diff --check", "ok",
        );
    }
    let errors = out
        .stdout
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with("warning:"))
        .count();
    let msg = if errors > 0 {
        format!("{errors} whitespace error(s)")
    } else {
        first_line(&out.stderr)
    };
    CheckResult::new(
        "whitespace", Stage::Convention, Status::Fail, Gate::Advisory, out.duration_ms,
        "git diff --check", &msg,
    )
    .with_output(&out.stdout)
}

/// Verify Signed-off-by trailers match author or committer (mirrors DCO.yml).
fn check_dco(scope: &ScopeInfo) -> CheckResult {
    if scope.mode != Mode::Diff || scope.base.is_empty() {
        return not_applicable("dco", "not applicable (files mode)");
    }
    let start = Instant::now();
    let mb = git_ok(&["merge-base", scope.base.as_str(), "HEAD"]);
    let commits: Vec<String> = git_ok(&["rev-list", &format!("{mb}..HEAD")])
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();

    let mut failed: Vec<String> = Vec::new();
    for c in &commits {
        // Skip merge commits.
        if git_ok(&["show", "-s", "--format=%P", c]).split_whitespace().count() > 1 {
            continue;
        }
        let ids = git_ok(&["show", "-s", "--format=%an%x00%ae%x00%cn%x00%ce", c]);
        let parts: Vec<&str> = ids.trim_end_matches(|c| c == '\n' || c == '\r').split('\0').collect();
        if parts.len() < 4 {
            failed.push(format!("{} (no identity)", &c[..c.len().min(7)]));
            continue;
        }
        let author = (parts[0].to_lowercase(), parts[1].to_lowercase());
        let committer = (parts[2].to_lowercase(), parts[3].to_lowercase());

        let msg = git_ok(&["show", "-s", "--format=%B", c]);
        let signed = msg.lines().any(|line| {
            let lowered = line.trim().to_lowercase();
            let Some(val) = lowered.strip_prefix("signed-off-by:") else {
                return false;
            };
            let val = val.trim();
            match (val.find('<'), val.rfind('>')) {
                (Some(lt), Some(gt)) if lt < gt => {
                    let s = (val[..lt].trim().to_lowercase(), val[lt + 1..gt].trim().to_lowercase());
                    s == author || s == committer
                }
                _ => false,
            }
        });
        if !signed {
            failed.push(c[..c.len().min(7)].to_string());
        }
    }

    let ms = start.elapsed().as_millis() as u64;
    if !failed.is_empty() {
        return CheckResult::new(
            "dco", Stage::Convention, Status::Fail, Gate::Advisory, ms,
            "internal: DCO check",
            &format!("{} of {} commit(s) missing valid Signed-off-by", failed.len(), commits.len()),
        )
        .with_output(&format!("Missing sign-off: {}", failed.join(", ")));
    }
    CheckResult::new(
        "dco", Stage::Convention, Status::Pass, Gate::Advisory, ms,
        "internal: DCO check",
        &format!("{} commit(s) verified", commits.len()),
    )
}

/// Library changes must include a CHANGELOG.md update (mirrors Changelog.yml).
fn check_changelog(scope: &ScopeInfo) -> CheckResult {
    if scope.mode != Mode::Diff || scope.base.is_empty() {
        return not_applicable("changelog", "not applicable (files mode)");
    }
    let mb = git_ok(&["merge-base", scope.base.as_str(), "HEAD"]);
    let changed: Vec<String> = must_git(&["diff", "--name-only", &format!("{mb}..HEAD")])
        .stdout
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .collect();

    let missing: Vec<String> = CHANGELOG_DIRS
        .iter()
        .filter(|d| {
            let prefix = format!("{d}/");
            changed.iter().any(|p| p.starts_with(&prefix))
                && !changed.iter().any(|p| p == &format!("{d}/CHANGELOG.md"))
        })
        .map(|d| format!("{d}/CHANGELOG.md"))
        .collect();

    let ms = 0; // covered by the git call above in the Python version; timing not meaningful here
    if !missing.is_empty() {
        return CheckResult::new(
            "changelog", Stage::Convention, Status::Fail, Gate::Advisory, ms,
            "internal: changelog check",
            &format!("{} changelog(s) not updated", missing.len()),
        )
        .with_output(&format!("Missing: {}", missing.join(", ")));
    }
    CheckResult::new(
        "changelog", Stage::Convention, Status::Pass, Gate::Advisory, ms,
        "internal: changelog check", "ok",
    )
}

/// Every changed text file should end with a newline (EOF newline).
fn check_eof_newline(scope: &ScopeInfo) -> CheckResult {
    let start = Instant::now();
    if scope.changed_paths.is_empty() {
        return not_applicable("eof-newline", "no changed files");
    }
    let mut missing: Vec<String> = Vec::new();
    for p in &scope.changed_paths {
        // Deleted or unreadable paths are skipped; empty files are fine.
        if let Ok(bytes) = std::fs::read(p) {
            if !bytes.is_empty() && bytes.last() != Some(&b'\n') {
                missing.push(p.clone());
            }
        }
    }
    let ms = start.elapsed().as_millis() as u64;
    if !missing.is_empty() {
        return CheckResult::new(
            "eof-newline", Stage::Convention, Status::Fail, Gate::Advisory, ms,
            "internal: EOF newline check",
            &format!("{} file(s) missing a newline at end of file", missing.len()),
        )
        .with_output(&format!("Missing EOF newline: {}", missing.join(", ")));
    }
    CheckResult::new(
        "eof-newline", Stage::Convention, Status::Pass, Gate::Advisory, ms,
        "internal: EOF newline check",
        &format!("{} file(s) checked", scope.changed_paths.len()),
    )
}

pub fn run_convention(scope: &ScopeInfo) -> Vec<CheckResult> {
    vec![
        check_fmt(),
        check_whitespace(scope),
        check_dco(scope),
        check_changelog(scope),
        check_eof_newline(scope),
    ]
}

