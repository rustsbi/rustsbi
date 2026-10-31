//! scope.rs — resolve the target and determine what to check.
//!
//! Parses the same raw argument string as resolve_target.py, extracts changed
//! paths, maps them to cargo packages, and computes a cache fingerprint.

use crate::gitutil::{git_ok, must_git};
use crate::model::{Mode, ScopeInfo};
use crate::util::{die, run_cmd};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::time::Duration;

/// Workspace-level files that force checking everything.
const WORKSPACE_FILES: [&str; 5] = [
    "Cargo.toml",
    "Cargo.lock",
    "rust-toolchain.toml",
    "rustfmt.toml",
    ".cargo/config.toml",
];

/// Quote-aware tokenizer for the raw argument string.
pub fn tokenize(raw: &str) -> Vec<String> {
    let mut toks = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    for ch in raw.chars() {
        match ch {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => {
                if !cur.is_empty() {
                    toks.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        toks.push(cur);
    }
    toks
}

fn is_pr_ref(s: &str) -> bool {
    let rest = s
        .strip_prefix("pr/")
        .or_else(|| s.strip_prefix("pull/"))
        .unwrap_or("");
    !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit())
}

/// (package name, package dir relative to the repo root) from cargo metadata.
fn cargo_metadata() -> Vec<(String, String)> {
    let out = run_cmd(
        "cargo",
        &["metadata", "--locked", "--no-deps", "--format-version", "1"],
        Duration::from_secs(30),
        &[],
    )
    .unwrap_or_else(|e| die(&format!("error: cargo metadata: {e}")));
    if !out.success {
        die(&format!(
            "error: cargo metadata failed: {}",
            crate::util::first_line(&out.stderr)
        ));
    }
    let Ok(cwd) = std::env::current_dir() else {
        die("error: cannot determine working directory");
    };
    let cwd = cwd.to_string_lossy().replace('\\', "/");

    let meta: Value = serde_json::from_str(&out.stdout)
        .unwrap_or_else(|e| die(&format!("error: cargo metadata JSON: {e}")));
    let mut pkgs = Vec::new();
    for p in meta["packages"].as_array().into_iter().flatten() {
        let name = p["name"].as_str().unwrap_or_default().to_string();
        let manifest = p["manifest_path"]
            .as_str()
            .unwrap_or_default()
            .replace('\\', "/");
        let mut pkg_dir = match manifest.rfind('/') {
            Some(i) => manifest[..i].to_string(),
            None => String::new(),
        };
        if let Some(rest) = pkg_dir.strip_prefix(&format!("{cwd}/")) {
            pkg_dir = rest.to_string();
        } else if pkg_dir == cwd {
            pkg_dir = String::new();
        }
        pkgs.push((name, pkg_dir));
    }
    pkgs
}

/// Map changed paths to cargo package names.
/// Returns (affected package names, check-everything flag).
pub fn map_paths_to_packages(changed: &[String], packages: &[(String, String)]) -> (Vec<String>, bool) {
    fn basename(p: &str) -> &str {
        p.rsplit(|c| c == '/' || c == '\\').next().unwrap_or(p)
    }
    for p in changed {
        if WORKSPACE_FILES.contains(&basename(p)) || WORKSPACE_FILES.contains(&p.as_str()) {
            return (packages.iter().map(|(n, _)| n.clone()).collect(), true);
        }
    }

    let cwd = std::env::current_dir()
        .map(|c| c.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let mut affected: BTreeSet<String> = BTreeSet::new();
    for path in changed {
        let norm = path.replace('\\', "/");
        let norm = norm.strip_prefix(&format!("{cwd}/")).unwrap_or(&norm);
        for (name, pkg_dir) in packages {
            if pkg_dir.is_empty() {
                continue; // workspace root
            }
            if norm == pkg_dir || norm.starts_with(&format!("{pkg_dir}/")) {
                affected.insert(name.clone());
                break;
            }
        }
    }
    (affected.into_iter().collect(), false)
}

/// Stable fingerprint (head, dirty state, packages, toolchain) for caching.
fn compute_fingerprint(scope: &ScopeInfo) -> String {
    let mut h = Sha256::new();
    h.update(scope.mode_str());
    h.update(&scope.head);
    h.update(scope.dirty.to_string());
    h.update(scope.affected_packages.join(","));
    for tool in ["rustc", "cargo"] {
        let v = run_cmd(tool, &["--version"], Duration::from_secs(10), &[])
            .map(|o| if o.success { o.stdout.trim().to_string() } else { String::new() })
            .unwrap_or_default();
        h.update(v);
    }
    let hex = format!("{:x}", h.finalize());
    hex[..16].to_string()
}

/// Parse raw args and resolve the scope. DIES on malformed input.
pub fn run_scope(raw_args: &str) -> ScopeInfo {
    let toks = tokenize(raw_args);
    if toks.len() < 3 {
        die("usage: diff <base|pr/N> <output> | files <path[:lines]> ... <output>");
    }
    let mode = toks[0].as_str();
    let targets = &toks[1..toks.len() - 1];

    if mode == "diff" {
        if !targets.is_empty() && is_pr_ref(&targets[0]) {
            let mut s = ScopeInfo::new(Mode::Pr);
            s.skipped_reason = "backtest: pr/N does not check out".to_string();
            return s;
        }
        let base = targets[0].clone();
        let head = git_ok(&["rev-parse", "--short", "HEAD"]);
        let branch = git_ok(&["rev-parse", "--abbrev-ref", "HEAD"]);
        let mb = git_ok(&["merge-base", &base, "HEAD"]);
        let range = format!("{mb}..HEAD");

        let changed: Vec<String> = must_git(&["diff", "--name-only", &range])
            .stdout
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        let dirty = !must_git(&["status", "--porcelain"]).stdout.trim().is_empty();

        let packages = cargo_metadata();
        let (affected, all_packages) = map_paths_to_packages(&changed, &packages);

        let mut scope = ScopeInfo::new(Mode::Diff);
        scope.base = base;
        scope.head = head;
        scope.branch = branch;
        scope.changed_paths = changed;
        scope.affected_packages = affected;
        scope.dirty = dirty;
        scope.all_packages = all_packages;
        scope.fingerprint = compute_fingerprint(&scope);
        scope
    } else if mode == "files" {
        let paths: Vec<String> = targets
            .iter()
            .map(|tok| tok.split(':').next().unwrap_or(tok).to_string())
            .filter(|p| std::path::Path::new(p).is_file())
            .collect();
        let packages = cargo_metadata();
        let (affected, all_packages) = map_paths_to_packages(&paths, &packages);

        let mut scope = ScopeInfo::new(Mode::Files);
        scope.head = git_ok(&["rev-parse", "--short", "HEAD"]);
        scope.changed_paths = paths;
        scope.affected_packages = affected;
        scope.all_packages = all_packages;
        scope.fingerprint = compute_fingerprint(&scope);
        scope
    } else {
        die(&format!("error: unknown mode: {mode}"))
    }
}
