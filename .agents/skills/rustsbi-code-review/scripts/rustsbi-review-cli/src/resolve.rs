//! resolve-target subcommand — build the canonical review input.
//! Rust port of scripts/resolve_target.py.
//!
//!     rustsbi-review-cli resolve-target '<raw args>'          # -> <output>.input.md
//!     rustsbi-review-cli resolve-target --meta '<raw args>'   # meta key=value lines
//!
//! Raw args:
//!     diff   <base|pr/N>        <output>
//!     files  <path[:lines] ...] <output>

use crate::gitutil::must_git;
use crate::util::{die, run_cmd};
use regex::Regex;
use serde_json::Value;
use std::time::Duration;

const FENCE: &str = "```";

fn range_re() -> Regex {
    Regex::new(r"^[0-9]+(-[0-9]+)?(,[0-9]+(-[0-9]+)?)*$").unwrap()
}
fn pr_ref_re() -> Regex {
    Regex::new(r"^(?:pr/|pull/)([0-9]+)(?:/head)?(?:@(.+))?").unwrap()
}
fn rev_re() -> Regex {
    Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._/-]*$").unwrap()
}

fn lang_for(path: &str) -> &'static str {
    match path.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
        Some("rs") => "rust",
        Some("md") => "markdown",
        Some("py") => "python",
        Some("sh") => "sh",
        Some("toml") => "toml",
        Some("yaml") | Some("yml") => "yaml",
        Some("json") => "json",
        _ => "",
    }
}

fn today() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

/// Run a command, dying on failure (Python resolve_target.run).
fn run_or_die(program: &str, args: &[&str]) -> String {
    match run_cmd(program, args, Duration::from_secs(600), &[]) {
        Ok(o) if o.success => o.stdout,
        Ok(o) => {
            let err = if o.stderr.trim().is_empty() { o.stdout.trim() } else { o.stderr.trim() };
            die(&format!("error: {} {}\n{}", program, args.join(" "), err))
        }
        Err(e) => die(&format!("error: {} {}: {}", program, args.join(" "), e)),
    }
}

fn parse_ranges(spec: &str) -> Vec<(u64, u64)> {
    spec.split(',')
        .map(|part| match part.split_once('-') {
            Some((a, b)) => (a.parse().unwrap_or(0), b.parse().unwrap_or(0)),
            None => {
                let n = part.parse().unwrap_or(0);
                (n, n)
            }
        })
        .collect()
}

fn split_target(tok: &str) -> (String, Option<Vec<(u64, u64)>>) {
    if let Some(idx) = tok.rfind(':') {
        let (path, spec) = (&tok[..idx], &tok[idx + 1..]);
        if !path.is_empty() && range_re().is_match(spec) {
            return (path.to_string(), Some(parse_ranges(spec)));
        }
    }
    (tok.to_string(), None)
}

fn repo_slug() -> String {
    if let Ok(r) = std::env::var("RCR_REPO") {
        return r;
    }
    run_or_die("gh", &["repo", "view", "--json", "nameWithOwner", "-q", ".nameWithOwner"])
        .trim()
        .to_string()
}

/// Rebuild a git-style patch from the compare API's per-file patches.
fn render_patch(files: &Value) -> String {
    let mut out = Vec::new();
    for f in files.as_array().into_iter().flatten() {
        // Python .rstrip(NL): strip trailing newlines only.
        let body = f["patch"].as_str().unwrap_or("").trim_end_matches(|c| c == '\n' || c == '\r');
        if body.is_empty() {
            continue;
        }
        out.push(format!("diff --git a/{} b/{}", f["filename"].as_str().unwrap_or(""), f["filename"].as_str().unwrap_or("")));
        out.push(body.to_string());
    }
    out.join("\n")
}

fn render_pr(num: &str, rev: Option<&str>) -> (String, Vec<String>) {
    let repo = repo_slug();
    let info: Value = serde_json::from_str(&run_or_die("gh", &[
        "pr", "view", num, "--repo", &repo, "--json",
        "number,title,baseRefName,baseRefOid,headRefOid,headRefName,commits",
    ]))
    .unwrap_or_else(|e| die(&format!("error: gh pr view: {e}")));

    let (commits, patch, head, source): (Value, String, String, String);
    if let Some(rev) = rev {
        // Pin a historical revision: review base...rev instead of the PR head.
        if !rev_re().is_match(rev) {
            die(&format!("error: unusable revision: {rev}"));
        }
        let base = info["baseRefOid"].as_str().unwrap_or("");
        let cmp_data: Value = serde_json::from_str(&run_or_die("gh", &[
            "api", &format!("repos/{}/compare/{}...{}", repo, base, rev),
        ]))
        .unwrap_or_else(|e| die(&format!("error: gh api compare: {e}")));
        let mut list = Vec::new();
        for c in cmp_data["commits"].as_array().into_iter().flatten() {
            let msg = c["commit"]["message"].as_str().unwrap_or("");
            let mut lines = msg.split('\n');
            let headline = lines.next().unwrap_or("").to_string();
            let body = lines.collect::<Vec<_>>().join("\n").trim().to_string();
            list.push(json_commit(c["sha"].as_str().unwrap_or(""), &headline, &body));
        }
        commits = Value::Array(list);
        patch = render_patch(&cmp_data["files"]);
        head = rev.chars().take(7).collect::<String>();
        source = format!("pr/{}@{}", num, rev);
    } else {
        commits = info["commits"].clone();
        patch = run_or_die("gh", &["pr", "diff", num, "--repo", &repo]);
        head = info["headRefOid"].as_str().unwrap_or("").chars().take(7).collect();
        source = format!("pr/{}", num);
    }

    let mut lines = vec![
        "# REVIEW INPUT".to_string(),
        String::new(),
        "mode: diff".to_string(),
        format!("source: {}", source),
        format!("title: {}", info["title"].as_str().unwrap_or("").trim()),
        format!("base: {} ({})", info["baseRefName"].as_str().unwrap_or(""), info["baseRefOid"].as_str().unwrap_or("").chars().take(8).collect::<String>()),
        format!("head: {}", head),
        format!("branch: {}", info["headRefName"].as_str().unwrap_or("")),
        String::new(),
        "## Commits, oldest first".to_string(),
        String::new(),
    ];
    for c in commits.as_array().into_iter().flatten() {
        lines.push(format!("### commit {}", c["oid"].as_str().unwrap_or("").chars().take(7).collect::<String>()));
        lines.push(c["messageHeadline"].as_str().unwrap_or("").trim().to_string());
        let body = c["messageBody"].as_str().unwrap_or("").trim();
        if !body.is_empty() {
            lines.push(String::new());
            lines.push(body.to_string());
        }
        lines.push(String::new());
    }
    lines.push("## Diff".to_string());
    lines.push(String::new());
    lines.push(format!("{}diff", FENCE));
    lines.push(patch.trim_end_matches(|c| c == '\n' || c == '\r').to_string());
    lines.push(FENCE.to_string());
    lines.push(String::new());

    let meta = vec![
        "mode=diff".to_string(),
        format!("base={}", source),
        format!("head={}", head),
        format!("branch={}", info["headRefName"].as_str().unwrap_or("")),
        format!("date={}", today()),
    ];
    (lines.join("\n"), meta)
}

fn json_commit(oid: &str, headline: &str, body: &str) -> Value {
    serde_json::json!({
        "oid": oid,
        "messageHeadline": headline,
        "messageBody": body,
    })
}

fn render_git(base: &str) -> (String, Vec<String>) {
    let head = must_git(&["rev-parse", "--short", "HEAD"]).stdout.trim().to_string();
    let branch = must_git(&["rev-parse", "--abbrev-ref", "HEAD"]).stdout.trim().to_string();
    let mb = must_git(&["merge-base", base, "HEAD"]).stdout.trim().to_string();
    let log = must_git(&[
        "log", "-p", "--reverse",
        "--format=### commit %h%n%s%n%b",
        &format!("{}..HEAD", mb),
    ])
    .stdout;
    let lines = vec![
        "# REVIEW INPUT".to_string(),
        String::new(),
        "mode: diff".to_string(),
        format!("base: {} ({})", base, mb.chars().take(8).collect::<String>()),
        format!("head: {}", head),
        format!("branch: {}", branch),
        String::new(),
        format!("{}diff", FENCE),
        log.trim_end_matches(|c| c == '\n' || c == '\r').to_string(),
        FENCE.to_string(),
        String::new(),
    ];
    let meta = vec![
        "mode=diff".to_string(),
        format!("base={}", base),
        format!("head={}", head),
        format!("branch={}", branch),
        format!("date={}", today()),
    ];
    (lines.join("\n"), meta)
}

fn render_files(targets: &[String]) -> (String, Vec<String>) {
    let mut out = vec![
        "# REVIEW INPUT".to_string(),
        String::new(),
        "mode: files".to_string(),
        String::new(),
    ];
    for tok in targets {
        let (path, ranges) = split_target(tok);
        let body = match std::fs::read(&path) {
            Ok(b) => String::from_utf8_lossy(&b).into_owned(),
            Err(_) => die(&format!("error: no such file: {}", path)),
        };
        let body: Vec<&str> = body.lines().collect();
        let mut keep = std::collections::HashSet::new();
        match &ranges {
            Some(rs) => {
                for (a, b) in rs {
                    for i in *a..=*b {
                        keep.insert(i);
                    }
                }
            }
            None => {
                for i in 1..=body.len() as u64 {
                    keep.insert(i);
                }
            }
        }
        let mut label = path.clone();
        if let Some(rs) = &ranges {
            let parts: Vec<String> = rs.iter().map(|(a, b)| {
                if a == b { format!("{}", a) } else { format!("{}-{}", a, b) }
            }).collect();
            label += &format!(":{}", parts.join(","));
        }
        let lang = if path.contains('.') { lang_for(&path) } else { "" };
        out.push(format!("## `{}`", label));
        out.push(String::new());
        out.push(format!("{}{}", FENCE, lang));
        for (i, text) in body.iter().enumerate() {
            let n = i as u64 + 1;
            if keep.contains(&n) {
                out.push(format!("{:5} | {}", n, text));
            }
        }
        out.push(FENCE.to_string());
        out.push(String::new());
    }
    let meta = vec![
        "mode=files".to_string(),
        format!("files={}", targets.join(" ")),
        format!("date={}", today()),
    ];
    (out.join("\n"), meta)
}

pub fn resolve_main(argv: &[String]) -> i32 {
    let mut rest = argv;
    let mut meta_only = false;
    if rest.first().map(String::as_str) == Some("--meta") {
        meta_only = true;
        rest = &rest[1..];
    }
    let raw = rest.first().map(String::as_str).unwrap_or("");
    let toks = crate::scope::tokenize(raw);
    if toks.len() < 3 {
        die("usage: diff <base|pr/N> <output> | files <path[:lines]> ... <output>");
    }

    let mode = toks[0].as_str();
    let output = toks.last().unwrap().clone();
    let targets = &toks[1..toks.len() - 1];

    let (text, meta) = if mode == "diff" {
        if let Some(m) = pr_ref_re().captures(&targets[0]) {
            let num = m.get(1).map(|g| g.as_str()).unwrap_or("");
            let rev = m.get(2).map(|g| g.as_str().to_string());
            let rev = rev.as_deref();
            render_pr(num, rev)
        } else {
            render_git(&targets[0])
        }
    } else if mode == "files" {
        render_files(targets)
    } else {
        die(&format!("error: unknown mode: {}", mode))
    };

    if meta_only {
        let mut all = meta;
        all.push(format!("output={}", output));
        println!("{}", all.join("\n"));
        return 0;
    }
    let dest = format!("{}.input.md", output);
    std::fs::write(&dest, text).unwrap_or_else(|e| die(&format!("error: write {}: {}", dest, e)));
    println!("{}", dest);
    0
}
