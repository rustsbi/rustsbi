// SPDX-License-Identifier: MulanPSL-2.0

mod git;
mod message;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    repo: PathBuf,
    message_file: PathBuf,
    expected_paths: Vec<String>,
    #[serde(default)]
    large_change_reason: Option<String>,
    #[serde(default)]
    intentional_artifacts: BTreeMap<String, String>,
}

#[derive(Serialize)]
struct Change {
    path: String,
    added: Option<u64>,
    deleted: Option<u64>,
}

#[derive(Serialize)]
struct Report {
    schema_version: u32,
    allowed: bool,
    repo: PathBuf,
    head: Option<String>,
    index_sha256: String,
    message_sha256: String,
    author: message::Identity,
    committer: message::Identity,
    changed_lines: u64,
    changes: Vec<Change>,
    large_change_reason: Option<String>,
    intentional_artifacts: BTreeMap<String, String>,
    errors: Vec<String>,
    manual_review: Vec<String>,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn resolve(parent: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.into()
    } else {
        parent.join(path)
    }
}

fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', '\0', ':'])
        && !path.starts_with('/')
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
}

fn suspect(path: &str) -> bool {
    let path = path.to_ascii_lowercase();
    path.split('/').any(|part| {
        matches!(
            part,
            "target"
                | "node_modules"
                | "__pycache__"
                | ".cache"
                | ".venv"
                | "qemu-logs"
                | ".ds_store"
                | "thumbs.db"
        ) || part.starts_with(".tmp")
            || part.starts_with(".env")
    }) || [
        ".tmp", ".temp", ".swp", ".swo", ".bak", ".pyc", ".pdb", ".log", ".o", ".obj", ".exe",
    ]
    .iter()
    .any(|suffix| path.ends_with(suffix))
        || path.ends_with('~')
}

pub fn check(request_path: &Path, output: &Path) -> Result<u8> {
    if env::var_os("GIT_INDEX_FILE").is_some() {
        return Err(
            "alternate GIT_INDEX_FILE is unsupported; check the normal checkout index".into(),
        );
    }
    let request_path = request_path.canonicalize()?;
    let request: Request = serde_json::from_slice(&fs::read(&request_path)?)?;
    let parent = request_path.parent().ok_or("request has no parent")?;
    let repo = resolve(parent, &request.repo).canonicalize()?;
    let root =
        PathBuf::from(git::text(&repo, &["rev-parse", "--show-toplevel"])?).canonicalize()?;
    if repo != root {
        return Err("repo must be the checkout root".into());
    }
    let message_path = resolve(parent, &request.message_file);
    let message_bytes = fs::read(&message_path)?;
    let message = std::str::from_utf8(&message_bytes)?;
    let author = message::Identity::parse(&git::text(&repo, &["var", "GIT_AUTHOR_IDENT"])?)?;
    let committer = message::Identity::parse(&git::text(&repo, &["var", "GIT_COMMITTER_IDENT"])?)?;
    let index = git::bytes(&repo, &["ls-files", "--stage", "-z"], None)?;
    let head = git::run(&repo, &["rev-parse", "--verify", "HEAD"], None)?;
    let head = head
        .status
        .success()
        .then(|| String::from_utf8_lossy(&head.stdout).trim().into());
    let mut errors = message::check(&repo, message, &author, &committer)?;
    if !git::bytes(&repo, &["ls-files", "--unmerged", "-z"], None)?.is_empty() {
        errors.push("resolve unmerged index entries before committing".into());
    }
    for marker in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
    ] {
        let marker_path = git::text(&repo, &["rev-parse", "--git-path", marker])?;
        if resolve(&repo, Path::new(&marker_path)).exists() {
            errors.push(format!(
                "in-progress {marker}: use the explicit recovery workflow"
            ));
        }
    }
    let mut expected = BTreeSet::new();
    for path in &request.expected_paths {
        if !valid_path(path) || !expected.insert(path.clone()) {
            return Err(format!("invalid or duplicate expected path: {path:?}").into());
        }
    }
    let stats = git::bytes(
        &repo,
        &[
            "diff",
            "--cached",
            "--numstat",
            "--no-renames",
            "--no-ext-diff",
            "--no-textconv",
            "--ignore-submodules=none",
            "-z",
            "--",
        ],
        None,
    )?;
    let mut changes = Vec::new();
    let mut changed_lines = 0_u64;
    for record in stats.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let mut fields = record.splitn(3, |b| *b == b'\t');
        let mut number = || -> Result<Option<u64>> {
            let field = fields.next().ok_or("invalid Git numstat")?;
            Ok(if field == b"-" {
                None
            } else {
                Some(std::str::from_utf8(field)?.parse()?)
            })
        };
        let added = number()?;
        let deleted = number()?;
        let path =
            std::str::from_utf8(fields.next().ok_or("missing Git numstat path")?)?.to_owned();
        changed_lines = changed_lines
            .checked_add(added.unwrap_or(0))
            .and_then(|n| n.checked_add(deleted.unwrap_or(0)))
            .ok_or("diff size overflow")?;
        if suspect(&path)
            && !request
                .intentional_artifacts
                .get(&path)
                .is_some_and(|s| !s.trim().is_empty())
        {
            errors.push(format!(
                "suspected cache/temporary artifact requires explicit reviewed intent: {path:?}"
            ));
        }
        changes.push(Change {
            path,
            added,
            deleted,
        });
    }
    let actual: BTreeSet<_> = changes.iter().map(|change| change.path.clone()).collect();
    if actual.is_empty() {
        errors.push("no staged changes".into());
    }
    for path in actual.difference(&expected) {
        errors.push(format!("unexpected staged path: {path:?}"));
    }
    for path in expected.difference(&actual) {
        errors.push(format!("expected path is not staged: {path:?}"));
    }
    for (path, reason) in &request.intentional_artifacts {
        if !actual.contains(path) || reason.trim().is_empty() {
            return Err(format!(
                "artifact exception must name a staged path and a concrete reason: {path:?}"
            )
            .into());
        }
    }
    if changed_lines > 10_000
        && !request
            .large_change_reason
            .as_ref()
            .is_some_and(|s| !s.trim().is_empty())
    {
        errors.push(format!(
            "{changed_lines} changed lines exceeds 10000; explicit developer intent is required"
        ));
    }
    let whitespace = git::run(
        &repo,
        &[
            "diff",
            "--cached",
            "--check",
            "--no-ext-diff",
            "--no-textconv",
            "--",
        ],
        None,
    )?;
    if !whitespace.status.success() {
        errors.push(format!(
            "staged whitespace check failed: {}{}",
            String::from_utf8_lossy(&whitespace.stdout),
            String::from_utf8_lossy(&whitespace.stderr)
        ));
    }
    for module in ["sbi-rt", "sbi-spec", "sbi-testing", "rustsbi", "macros"] {
        let prefix = format!("library/{module}/");
        let changelog = format!("{prefix}CHANGELOG.md");
        if actual.iter().any(|path| path.starts_with(&prefix))
            && (!actual.contains(&changelog)
                || !git::run(&repo, &["cat-file", "-e", &format!(":{changelog}")], None)?
                    .status
                    .success())
        {
            errors.push(format!(
                "{changelog} must be included and present in the index (Changelog.yml)"
            ));
        }
    }
    if index != git::bytes(&repo, &["ls-files", "--stage", "-z"], None)? {
        errors.push(
            "index changed during validation; rerun with no concurrent Git operations".into(),
        );
    }
    let report = Report {
        schema_version: 1, allowed: errors.is_empty(), repo, head,
        index_sha256: hash(&index), message_sha256: hash(&message_bytes), author, committer,
        changed_lines, changes, large_change_reason: request.large_change_reason,
        intentional_artifacts: request.intentional_artifacts, errors,
        manual_review: vec![
            "Confirm every staged hunk belongs to the requested change; path matching cannot prove intent.".into(),
            "Inspect binary files and gitlinks separately; their contents have no text line count.".into(),
            "Confirm English prose, imperative subject, what/why/how, and truthful validation claims.".into(),
            "Any exception reason must come from explicit developer intent, not the agent's inference.".into(),
        ],
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&serde_json::to_vec_pretty(&report)?)?;
    file.write_all(b"\n")?;
    println!(
        "{}: {} changed lines; report {}",
        if report.allowed { "PASS" } else { "BLOCKED" },
        changed_lines,
        output.display()
    );
    for error in &report.errors {
        eprintln!("- {error}");
    }
    Ok(if report.allowed { 0 } else { 1 })
}
