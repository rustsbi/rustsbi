//! Small process/IO helpers shared by every stage.

use anyhow::{anyhow, Result};
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use wait_timeout::ChildExt;

/// Outcome of one command run. `success` is false on non-zero exit, timeout,
/// or termination by signal; callers decide what that means.
pub struct CmdOutcome {
    pub timed_out: bool,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// Run a command with a timeout, capturing output on reader threads so a
/// chatty child can never deadlock on a full pipe. Never fails on non-zero
/// exit; only spawn errors propagate.
pub fn run_cmd(
    program: &str,
    args: &[&str],
    timeout: Duration,
    env_extra: &[(String, String)],
) -> Result<CmdOutcome> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.envs(env_extra.iter().cloned());

    let start = Instant::now();
    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow!("cannot run `{program}`: {e}"))?;

    fn reader<R: Read + Send + 'static>(r: R) -> std::thread::JoinHandle<Vec<u8>> {
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut r = r;
            let _ = r.read_to_end(&mut buf);
            buf
        })
    }
    let out_handle = child.stdout.take().map(reader);
    let err_handle = child.stderr.take().map(reader);

    let mut timed_out = false;
    let success = match child.wait_timeout(timeout)? {
        Some(status) => status.success(),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            timed_out = true;
            false
        }
    };

    let decode = |h: Option<std::thread::JoinHandle<Vec<u8>>>| {
        h.map(|h| h.join().unwrap_or_default())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    };

    Ok(CmdOutcome {
        timed_out,
        success,
        stdout: decode(out_handle),
        stderr: decode(err_handle),
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

/// Run a command or die loudly (missing tool, permission, ...).
pub fn must_run(program: &str, args: &[&str], timeout: Duration) -> CmdOutcome {
    run_cmd(program, args, timeout, &[]).unwrap_or_else(|e| die(&format!("error: {e}")))
}

impl CmdOutcome {
    /// Status implied by this outcome: pass, timeout, or fail-ish (caller
    /// refines fail vs error using the output).
    pub fn status(&self) -> crate::model::Status {
        if self.timed_out {
            crate::model::Status::Timeout
        } else if self.success {
            crate::model::Status::Pass
        } else {
            crate::model::Status::Fail
        }
    }
}

/// Python-style first non-empty line, truncated to 200 chars.
pub fn first_line(text: &str) -> String {
    for line in text.lines() {
        let line = line.trim();
        if !line.is_empty() {
            return line.chars().take(200).collect();
        }
    }
    String::new()
}

/// Truncate to a char-safe limit with an explicit marker.
pub fn truncate(text: &str, limit: usize) -> String {
    clip(text, limit)
        .map(|s| format!("{s}\n... (truncated)"))
        .unwrap_or_else(|| text.to_string())
}

/// Char-safe slice of at most `limit` bytes; None when already short enough.
pub fn clip(text: &str, limit: usize) -> Option<&str> {
    if text.len() <= limit {
        return None;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    Some(&text[..end])
}

pub fn die(msg: &str) -> ! {
    eprintln!("{}", msg.trim_end());
    std::process::exit(2);
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
}

/// Look for an executable on PATH and in ~/.cargo/bin.
pub fn find_tool(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    if let Some(home) = home_dir() {
        let p = home.join(".cargo").join("bin").join(&exe);
        if p.is_file() {
            return Some(p);
        }
    }
    if let Ok(path) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path) {
            let p = dir.join(&exe);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

/// ~/.cargo/bin, when it exists.
pub fn cargo_bin() -> Option<PathBuf> {
    home_dir()
        .map(|h| h.join(".cargo").join("bin"))
        .filter(|p| p.is_dir())
}

/// Env addition that puts ~/.cargo/bin first on PATH.
pub fn cargo_bin_env() -> Vec<(String, String)> {
    match cargo_bin() {
        Some(bin) => {
            let sep = if cfg!(windows) { ";" } else { ":" };
            vec![(
                "PATH".to_string(),
                format!("{}{}{}", bin.to_string_lossy(), sep, std::env::var("PATH").unwrap_or_default()),
            )]
        }
        None => Vec::new(),
    }
}
