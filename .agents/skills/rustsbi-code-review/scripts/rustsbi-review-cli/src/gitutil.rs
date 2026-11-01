//! Git invocation helpers, worktree-aware.
//!
//! In a linked worktree `.git` is a file pointing at the main repo's gitdir.
//! On Windows this resolves natively; on WSL an `E:/` path does not, so we
//! export GIT_DIR/GIT_WORK_TREE explicitly with the path converted to the
//! /mnt/<drive>/ mount.

use crate::util::{run_cmd, CmdOutcome};
use anyhow::Result;
use std::time::Duration;

pub fn git_env() -> Vec<(String, String)> {
    let mut env = Vec::new();
    let dotgit = std::path::Path::new(".git");
    if !dotgit.is_file() {
        return env;
    }
    let Ok(content) = std::fs::read_to_string(dotgit) else {
        return env;
    };
    let Some(rest) = content.trim().strip_prefix("gitdir:") else {
        return env;
    };
    let mut gitdir = rest.trim().to_string();
    if is_windows_drive(&gitdir) {
        let drive = gitdir.as_bytes()[0].to_ascii_lowercase();
        let tail = gitdir[3..].replace('\\', "/");
        gitdir = format!("/mnt/{}/{}", drive as char, tail);
    }
    env.push(("GIT_DIR".to_string(), gitdir));
    if let Ok(cwd) = std::env::current_dir() {
        env.push(("GIT_WORK_TREE".to_string(), cwd.to_string_lossy().into_owned()));
    }
    env
}

fn is_windows_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && (b[2] == b'/' || b[2] == b'\\')
}

/// Run a git command with the worktree-aware environment.
pub fn git(args: &[&str]) -> Result<CmdOutcome> {
    let mut full: Vec<&str> = vec!["git"];
    // -c core.autocrlf=true handles CRLF on Windows/WSL for diff/status.
    if matches!(args.first(), Some(&"diff") | Some(&"status")) {
        full.extend(["-c", "core.autocrlf=true"]);
    }
    full.extend_from_slice(args);
    let env = git_env();
    run_cmd(full[0], &full[1..], Duration::from_secs(600), &env)
}

/// Run git and die with a clear message on failure.
pub fn must_git(args: &[&str]) -> CmdOutcome {
    git(args).unwrap_or_else(|e| crate::util::die(&format!("error: git {} failed: {e}", args.join(" "))))
}

/// Run git, returning trimmed stdout on success, dying otherwise.
pub fn git_ok(args: &[&str]) -> String {
    let out = must_git(args);
    if !out.success {
        crate::util::die(&format!(
            "error: git {} failed: {}",
            args.join(" "),
            crate::util::first_line(&out.stderr)
        ));
    }
    out.stdout.trim().to_string()
}
