//! QEMU execution for kernel-backed prototyper commands.
//!
//! Boots a payload-mode firmware ELF under `qemu-system-riscv64` and
//! verifies the captured console output against per-kernel expectations.
//! Mirrors the payload branch of `.github/scripts/prototyper-qemu-boot.sh`.

use std::{
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

/// Poll interval while waiting for a QEMU process to exit.
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Number of console lines printed when a run fails.
const LOG_TAIL_LINES: usize = 120;
/// Substrings that mark a failed run even when QEMU exits successfully.
const FORBIDDEN_PATTERNS: &[&str] = &["panic", "FAILED", "SystemFailure"];

/// A QEMU boot to execute and verify.
pub(super) struct QemuRun {
    /// Firmware ELF passed as `-bios`.
    pub bios: PathBuf,
    /// Number of harts (`-smp`).
    pub smp: usize,
    /// Timeout of one attempt.
    pub timeout: Duration,
    /// Total attempts; retries happen only after a timeout.
    pub attempts: usize,
    /// Console substrings that must all be present for the run to pass.
    pub expected: Vec<String>,
    /// Human readable label used in log messages (e.g. `test`).
    pub label: String,
}

/// Outcome of one QEMU attempt.
enum Attempt {
    /// QEMU exited before the timeout; carries exit success and console output.
    Exited { success: bool, output: String },
    /// QEMU was killed after the timeout elapsed.
    TimedOut { output: String },
}

/// Boot `run.bios` in QEMU and verify the kernel console output.
///
/// Retries are only performed when an attempt times out; a clean QEMU exit
/// with failing output verification fails immediately.
pub(super) fn run(run: &QemuRun) -> Result<()> {
    if run.attempts == 0 {
        bail!("QEMU attempts must be at least 1 (got --retries 0)");
    }
    if run.smp == 0 {
        bail!("QEMU hart count must be at least 1 (got --smp 0)");
    }

    info!(
        "Running {} kernel in QEMU (smp={}, timeout={}s, attempts={})",
        run.label,
        run.smp,
        run.timeout.as_secs(),
        run.attempts
    );

    for attempt in 1..=run.attempts {
        match run_once(run)? {
            Attempt::Exited {
                success: true,
                output,
            } => match verify_output(&output, &run.expected) {
                Ok(()) => {
                    info!(
                        "{} kernel run passed on attempt {}/{}",
                        run.label, attempt, run.attempts
                    );
                    print_results(&run.label, &output, &run.expected);
                    return Ok(());
                }
                Err(err) => {
                    print_tail(&run.label, &output);
                    return Err(err);
                }
            },
            Attempt::Exited {
                success: false,
                output,
            } => {
                print_tail(&run.label, &output);
                bail!(
                    "QEMU exited with a non-zero status while running the {} kernel",
                    run.label
                );
            }
            Attempt::TimedOut { output } => {
                if attempt < run.attempts {
                    warn!(
                        "QEMU timed out after {}s on attempt {}/{}; retrying",
                        run.timeout.as_secs(),
                        attempt,
                        run.attempts
                    );
                } else {
                    print_tail(&run.label, &output);
                    bail!(
                        "QEMU timed out after {}s while running the {} kernel ({} attempt(s))",
                        run.timeout.as_secs(),
                        run.label,
                        run.attempts
                    );
                }
            }
        }
    }

    unreachable!("attempts >= 1, so the loop always returns or bails")
}

/// Spawn one QEMU process, enforce the timeout, and capture console output.
fn run_once(run: &QemuRun) -> Result<Attempt> {
    let mut child = Command::new("qemu-system-riscv64")
        .args([
            "-machine",
            "virt",
            "-m",
            "256M",
            "-smp",
            &run.smp.to_string(),
            "-nographic",
            "-bios",
        ])
        .arg(&run.bios)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context(
            "failed to execute qemu-system-riscv64; please install QEMU \
             (e.g. `sudo apt install qemu-system-misc` on Debian/Ubuntu) \
             and make sure qemu-system-riscv64 is on PATH",
        )?;

    let deadline = Instant::now() + run.timeout;
    let timed_out = loop {
        match child.try_wait() {
            Ok(Some(_)) => break false,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    break true;
                }
                thread::sleep(POLL_INTERVAL);
            }
            Err(err) => {
                let _ = child.kill();
                return Err(err).context("failed to wait on the QEMU process");
            }
        }
    };

    let output = child
        .wait_with_output()
        .context("failed to read QEMU console output")?;
    let mut console = String::from_utf8_lossy(&output.stdout).into_owned();
    console.push_str(&String::from_utf8_lossy(&output.stderr));

    if timed_out {
        Ok(Attempt::TimedOut { output: console })
    } else {
        Ok(Attempt::Exited {
            success: output.status.success(),
            output: console,
        })
    }
}

/// Verify captured console output: all `expected` patterns must be present
/// and no failure pattern may appear.
pub(super) fn verify_output(output: &str, expected: &[String]) -> Result<()> {
    if output.trim().is_empty() {
        bail!("QEMU produced no console output");
    }

    let missing: Vec<&str> = expected
        .iter()
        .filter(|pattern| !output.contains(pattern.as_str()))
        .map(String::as_str)
        .collect();
    if !missing.is_empty() {
        bail!(
            "kernel output is missing expected pattern(s): {}",
            missing.join(", ")
        );
    }

    if let Some(pattern) = FORBIDDEN_PATTERNS
        .iter()
        .find(|pattern| output.contains(**pattern))
    {
        bail!("kernel output contains failure pattern `{pattern}`");
    }

    Ok(())
}

/// Print the console lines carrying the kernel's results (e.g. test passes,
/// benchmark numbers): lines matching any expected pattern.
fn print_results(label: &str, output: &str, expected: &[String]) {
    println!("----- {label} kernel results -----");
    for line in output.lines() {
        if expected
            .iter()
            .any(|pattern| line.contains(pattern.as_str()))
        {
            println!("{line}");
        }
    }
    println!("----- end of {label} kernel results -----");
}

/// Print the tail of the captured console output for post-mortem analysis.
fn print_tail(label: &str, output: &str) {
    let lines: Vec<&str> = output.lines().collect();
    let skip = lines.len().saturating_sub(LOG_TAIL_LINES);
    eprintln!(
        "----- {label} kernel QEMU console output (last {} of {} lines) -----",
        lines.len() - skip,
        lines.len()
    );
    for line in &lines[skip..] {
        eprintln!("{line}");
    }
    eprintln!("----- end of {label} kernel QEMU console output -----");
}
