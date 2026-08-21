//! QEMU execution for kernel-backed prototyper commands.
//!
//! Boots a payload-mode firmware ELF under `qemu-system-riscv64` and
//! verifies the captured console output against per-kernel expectations.
//! Mirrors the payload branch of `.github/scripts/prototyper-qemu-boot.sh`.

use std::{
    io::Read,
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};

/// Poll interval while waiting for a QEMU process to exit.
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// Number of console lines printed when a run fails.
const LOG_TAIL_LINES: usize = 120;

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
    /// Console substrings that mark a failed run even when QEMU exits
    /// successfully (e.g. the `panicked at` prefix of Rust panic messages;
    /// the shorter `panic` would false-positive on legitimate output
    /// mentioning panics).
    pub forbidden: Vec<String>,
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
            } => match verify_output(&output, &run.expected, &run.forbidden) {
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
///
/// stdout and stderr are drained on dedicated threads while the child runs;
/// reading only after exit would deadlock once QEMU fills the pipe buffer.
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

    let stdout_reader = spawn_stream_reader(&mut child, Stream::Stdout);
    let stderr_reader = spawn_stream_reader(&mut child, Stream::Stderr);

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

    let status = child.wait().context("failed to wait on the QEMU process")?;
    let stdout = join_stream_reader(stdout_reader, "stdout")?;
    let stderr = join_stream_reader(stderr_reader, "stderr")?;
    let mut console = String::from_utf8_lossy(&stdout).into_owned();
    console.push_str(&String::from_utf8_lossy(&stderr));

    if timed_out {
        Ok(Attempt::TimedOut { output: console })
    } else {
        Ok(Attempt::Exited {
            success: status.success(),
            output: console,
        })
    }
}

/// Which child stream a reader thread drains.
enum Stream {
    Stdout,
    Stderr,
}

/// Spawn a thread that reads the given child stream to EOF.
fn spawn_stream_reader(
    child: &mut Child,
    stream: Stream,
) -> thread::JoinHandle<std::io::Result<Vec<u8>>> {
    let mut pipe: Box<dyn Read + Send> = match stream {
        Stream::Stdout => Box::new(child.stdout.take().expect("child stdout was piped")),
        Stream::Stderr => Box::new(child.stderr.take().expect("child stderr was piped")),
    };
    thread::spawn(move || {
        let mut buffer = Vec::new();
        pipe.read_to_end(&mut buffer)?;
        Ok(buffer)
    })
}

/// Join a stream reader thread and return the captured bytes.
fn join_stream_reader(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    name: &str,
) -> Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("QEMU {name} reader thread panicked"))?
        .with_context(|| format!("failed to read QEMU {name}"))
}

/// Verify captured console output: all `expected` patterns must be present
/// and no `forbidden` pattern may appear.
pub(super) fn verify_output(output: &str, expected: &[String], forbidden: &[String]) -> Result<()> {
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

    if let Some(pattern) = forbidden
        .iter()
        .find(|pattern| output.contains(pattern.as_str()))
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
