//! QEMU execution and `mbench` capture verification for prototyper `test` and `bench`.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::utils::workspace_root;

use super::kernels::Kernel;

const POLL_INTERVAL: Duration = Duration::from_millis(50);
const LOG_TAIL_LINES: usize = 120;
const QEMU_MEMORY_MB: u64 = 256;

#[derive(Debug, Clone)]
pub(super) enum QemuPayload {
    Payload,
    Dynamic {
        kernel_elf: PathBuf,
        bootargs: &'static str,
    },
    Jump {
        kernel_bin: PathBuf,
        load_addr: u64,
    },
}

/// A QEMU boot to execute and verify.
pub(super) struct QemuRun {
    pub bios: PathBuf,
    pub payload: QemuPayload,
    pub mbench_cli: PathBuf,
    pub work_dir: PathBuf,
    pub kernel: Kernel,
    pub machine: String,
    pub smp: usize,
    pub timeout: Duration,
    /// Total attempts; retries happen only after a timeout without observed failure.
    pub attempts: usize,
    /// Console substrings that fail the run even when QEMU exits cleanly or times out.
    pub forbidden: Vec<String>,
    pub out: Option<PathBuf>,
}

enum Attempt {
    Exited { success: bool, output: String },
    TimedOut { output: String },
}

/// Build a distinct per-attempt file path beside `base` so retried timeouts preserve prior logs.
fn attempt_capture_path(base: &Path, attempt: usize) -> PathBuf {
    let suffix = format!("attempt-{attempt}");
    match (base.file_stem(), base.extension()) {
        (Some(stem), Some(ext)) => {
            let mut file_name = stem.to_os_string();
            file_name.push(format!(".{suffix}."));
            file_name.push(ext);
            base.with_file_name(file_name)
        }
        (Some(stem), None) => {
            let mut file_name = stem.to_os_string();
            file_name.push(format!(".{suffix}"));
            base.with_file_name(file_name)
        }
        _ => base.with_extension(suffix),
    }
}

/// Boot `run.bios` in QEMU, capture serial output, and verify the capture via `mbench validate --json`.
pub(super) fn run(run: &QemuRun) -> Result<()> {
    let label = run.kernel.command_name();
    info!(
        "Running {label} in QEMU (machine={}, smp={}, timeout={}s, attempts={})",
        run.machine,
        run.smp,
        run.timeout.as_secs(),
        run.attempts
    );

    for attempt in 1..=run.attempts {
        match run_once(run)? {
            Attempt::Exited {
                success: true,
                output,
            } => {
                let capture_path = match &run.out {
                    Some(path) => path.clone(),
                    None => run
                        .work_dir
                        .join(format!("qemu-{label}-attempt-{attempt}.log")),
                };
                write_capture_file(&capture_path, &output)?;
                let verified = verify_capture(&output, &capture_path, run);
                if run.out.is_none() && verified.is_ok() {
                    let _ = fs::remove_file(&capture_path);
                }
                match verified {
                    Ok(()) => {
                        info!("{label} run passed on attempt {attempt}/{}", run.attempts);
                        return Ok(());
                    }
                    Err(err) => {
                        print_tail(label, &output);
                        return Err(err.context(format!(
                            "full QEMU capture saved to '{}'",
                            capture_path.display()
                        )));
                    }
                }
            }
            Attempt::Exited {
                success: false,
                output,
            } => {
                save_optional_output(run.out.as_deref(), &output);
                print_tail(label, &output);
                bail!("QEMU exited with a non-zero status while running {label}");
            }
            Attempt::TimedOut { output } => {
                let is_final = attempt == run.attempts;
                let capture_path = match run.out.as_deref() {
                    Some(path) if is_final => path.to_path_buf(),
                    Some(path) => attempt_capture_path(path, attempt),
                    None => run
                        .work_dir
                        .join(format!("qemu-timeout-{label}-attempt-{attempt}.log")),
                };
                let write_res = write_capture_file(&capture_path, &output);
                let saved = if is_final {
                    write_res.is_ok()
                } else {
                    write_res?;
                    true
                };
                if saved
                    && let Err(err) = check_timeout_for_explicit_failure(
                        &output,
                        &capture_path,
                        &run.mbench_cli,
                        &run.forbidden,
                    )
                {
                    if !is_final {
                        save_optional_output(run.out.as_deref(), &output);
                    }
                    print_tail(label, &output);
                    let detail = if is_final {
                        format!(
                            "QEMU timed out after {}s while running {label} after recording a failure (saved to '{}')",
                            run.timeout.as_secs(),
                            capture_path.display()
                        )
                    } else {
                        format!(
                            "timed-out attempt {attempt}/{} had already recorded a failure (saved to '{}')",
                            run.attempts,
                            capture_path.display()
                        )
                    };
                    return Err(err.context(detail));
                }
                if !is_final {
                    warn!(
                        "QEMU timed out after {}s on attempt {attempt}/{} (saved to '{}'); retrying",
                        run.timeout.as_secs(),
                        run.attempts,
                        capture_path.display()
                    );
                    continue;
                }
                print_tail(label, &output);
                bail!(
                    "QEMU timed out after {}s while running {label} ({} attempt(s); capture: '{}')",
                    run.timeout.as_secs(),
                    run.attempts,
                    capture_path.display()
                );
            }
        }
    }

    unreachable!("attempts >= 1, so the loop always returns or bails")
}

#[derive(Deserialize)]
struct CaptureReport {
    protocol_valid: bool,
    completed: bool,
    #[serde(default)]
    protocol_errors: Vec<String>,
    meta: Option<ReportMeta>,
    panic: Option<serde_json::Value>,
    counts: ReportCounts,
    #[serde(default)]
    checks: Vec<ReportItem>,
    #[serde(default)]
    benches: Vec<ReportItem>,
}

#[derive(Deserialize)]
struct ReportMeta {
    hart_count: usize,
    #[serde(default)]
    extensions: Vec<String>,
    config: Option<ReportConfig>,
}

#[derive(Deserialize)]
struct ReportConfig {
    mode: String,
}

#[derive(Deserialize)]
struct ReportCounts {
    checks_total: usize,
    checks_passed: usize,
    checks_skipped: usize,
    checks_failed: usize,
    check_assertions_failed: u64,
    benches_total: usize,
    benches_effective: usize,
    benches_skipped: usize,
    benches_failed: usize,
    quarantine_warnings: usize,
}

#[derive(Deserialize)]
struct ReportItem {
    id: String,
    status: String,
}

const REQUIRED_VIRT_EXTENSIONS: &[&str] = &[
    "base", "time", "spi", "rfnc", "hsm", "srst", "pmu", "dbcn", "susp", "cppc", "sta", "fwft",
    "dbtr", "mpxy",
];

const REQUIRED_VIRT_CHECKS: &[&str] = &[
    "conformance/base",
    "conformance/time",
    "conformance/spi",
    "conformance/rfnc",
    "conformance/pmu",
    "conformance/dbcn",
    "conformance/fwft",
    "conformance/dbtr",
    "conformance/shmem_security.sta",
    "conformance/shmem_security.dbtr",
    "conformance/shmem_security.mpxy",
    "conformance/system",
    "conformance/traps",
];

const REQUIRED_VIRT_SMP_CHECKS: &[&str] = &[
    "conformance/spi_smp",
    "conformance/rfnc_smp",
    "conformance/hsm",
];

const REQUIRED_VIRT_BENCHES: &[&str] = &[
    "base/get_spec_version",
    "base/probe.hit",
    "time/set_timer",
    "pmu/num_counters",
    "hsm/get_status",
];

const REQUIRED_VIRT_SMP_BENCHES: &[&str] = &[
    "spi/send_ipi.one",
    "rfnc/sfence_vma.all",
    "hsm/start_stop",
    "scale/base_ecall",
];

fn is_virt_machine(machine: &str) -> bool {
    machine == "virt" || machine.starts_with("virt,")
}

fn is_effective_bench_status(status: &str) -> bool {
    matches!(status, "PASS" | "HW_NATIVE" | "DELEGATED_TO_S")
}

fn run_mbench_validate(
    mbench_cli: &Path,
    capture_path: &Path,
) -> Result<(bool, CaptureReport, String)> {
    let output = Command::new(mbench_cli)
        .arg("validate")
        .arg(capture_path)
        .arg("--json")
        .output()
        .with_context(|| format!("failed to execute '{}'", mbench_cli.display()))?;
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    let report: CaptureReport = serde_json::from_slice(&output.stdout).with_context(|| {
        format!(
            "failed to parse JSON from `{} validate {} --json` (exit={:?}, stderr={stderr:?})",
            mbench_cli.display(),
            capture_path.display(),
            output.status.code()
        )
    })?;
    Ok((output.status.success(), report, stderr))
}

fn check_forbidden_patterns(output: &str, forbidden: &[String]) -> Result<()> {
    if let Some(pattern) = forbidden
        .iter()
        .find(|pattern| output.contains(pattern.as_str()))
    {
        bail!("console output contains forbidden pattern `{pattern}`");
    }
    Ok(())
}

fn ensure_no_report_failures(report: &CaptureReport, when: &str) -> Result<()> {
    if let Some(panic) = &report.panic {
        bail!("mbench target panicked{when}: {panic}");
    }
    if let Some(failed) = report.checks.iter().find(|check| check.status == "FAIL") {
        bail!("mbench check `{}` failed{when}", failed.id);
    }
    if let Some(failed) = report.benches.iter().find(|bench| bench.status == "FAIL") {
        bail!("mbench benchmark `{}` failed{when}", failed.id);
    }
    if report.counts.checks_failed > 0 || report.counts.check_assertions_failed > 0 {
        bail!(
            "mbench reported {} failed check(s) / {} failed assertion(s){when}",
            report.counts.checks_failed,
            report.counts.check_assertions_failed
        );
    }
    if report.counts.benches_failed > 0 {
        bail!(
            "mbench reported {} failed benchmark(s){when}",
            report.counts.benches_failed
        );
    }
    Ok(())
}

fn check_timeout_for_explicit_failure(
    output: &str,
    capture_path: &Path,
    mbench_cli: &Path,
    forbidden: &[String],
) -> Result<()> {
    check_forbidden_patterns(output, forbidden)?;
    if !output.contains("@@MBENCH:") {
        return Ok(());
    }
    match run_mbench_validate(mbench_cli, capture_path) {
        Ok((_, report, _)) => ensure_no_report_failures(&report, " before timeout"),
        Err(_) if output.contains("@@MBENCH:PANIC") => {
            bail!("mbench target emitted @@MBENCH:PANIC before timeout");
        }
        Err(err) => Err(err),
    }
}

fn check_required_items<'a>(
    kind: &str,
    expected: &str,
    base: &[&'a str],
    smp_extra: &[&'a str],
    smp: usize,
    items: &[ReportItem],
    accepts: impl Fn(&str) -> bool,
) -> Result<()> {
    let required = base
        .iter()
        .chain((smp >= 2).then_some(smp_extra).into_iter().flatten());
    for item_id in required {
        let status = items
            .iter()
            .find(|item| item.id == *item_id)
            .map(|item| item.status.as_str());
        if !status.is_some_and(&accepts) {
            bail!(
                "required QEMU virt {kind} `{item_id}` status is {status:?}, expected {expected}"
            );
        }
    }
    Ok(())
}

/// Verify captured QEMU console output via `mbench validate --json` and RustSBI platform policies.
fn verify_capture(output: &str, capture_path: &Path, run: &QemuRun) -> Result<()> {
    if output.trim().is_empty() {
        bail!("QEMU produced no console output");
    }
    check_forbidden_patterns(output, &run.forbidden)?;

    let (validate_ok, report, stderr) = run_mbench_validate(&run.mbench_cli, capture_path)?;
    if !validate_ok || !report.protocol_valid || !report.completed {
        let details = if !report.protocol_errors.is_empty() {
            report.protocol_errors.join("; ")
        } else if !stderr.is_empty() {
            stderr
        } else {
            format!(
                "protocol_valid={}, completed={}",
                report.protocol_valid, report.completed
            )
        };
        bail!("mbench protocol audit failed: {details}");
    }

    let meta = report
        .meta
        .as_ref()
        .context("mbench capture report missing metadata")?;
    if meta.hart_count != run.smp {
        bail!(
            "reported hart count {} does not match expected --smp {}",
            meta.hart_count,
            run.smp
        );
    }
    let expected_mode = match run.kernel {
        Kernel::Test => "check",
        Kernel::Bench => "bench",
    };
    let actual_mode = meta.config.as_ref().map(|c| c.mode.as_str());
    if actual_mode != Some(expected_mode) {
        bail!("@@MBENCH:META.config.mode is {actual_mode:?}, expected {expected_mode:?}");
    }

    ensure_no_report_failures(&report, "")?;
    if report.counts.quarantine_warnings != 0 {
        bail!(
            "mbench reported {} quarantine warning(s)",
            report.counts.quarantine_warnings
        );
    }

    if is_virt_machine(&run.machine) {
        for ext in REQUIRED_VIRT_EXTENSIONS {
            if !meta.extensions.iter().any(|present| present == ext) {
                bail!("missing expected SBI extension `{ext}` on QEMU virt");
            }
        }
        match run.kernel {
            Kernel::Test => check_required_items(
                "conformance check",
                "\"PASS\"",
                REQUIRED_VIRT_CHECKS,
                REQUIRED_VIRT_SMP_CHECKS,
                run.smp,
                &report.checks,
                |s| s == "PASS",
            )?,
            Kernel::Bench => check_required_items(
                "benchmark",
                "an effective run",
                REQUIRED_VIRT_BENCHES,
                REQUIRED_VIRT_SMP_BENCHES,
                run.smp,
                &report.benches,
                is_effective_bench_status,
            )?,
        }
    }

    match run.kernel {
        Kernel::Test if report.counts.checks_passed == 0 => {
            bail!("mbench reported 0 passed conformance checks");
        }
        Kernel::Bench if report.counts.benches_effective == 0 => {
            bail!("mbench reported 0 effective benchmarks");
        }
        Kernel::Test => println!(
            "[mbench] PASS test: harts={} checks={}/{} (skipped={})",
            meta.hart_count,
            report.counts.checks_passed,
            report.counts.checks_total,
            report.counts.checks_skipped
        ),
        Kernel::Bench => println!(
            "[mbench] PASS bench: harts={} effective={}/{} (skipped={})",
            meta.hart_count,
            report.counts.benches_effective,
            report.counts.benches_total,
            report.counts.benches_skipped
        ),
    }
    Ok(())
}

/// Console substrings that mark a failed QEMU boot even when the process
/// exits successfully, read from `firmware/scripts/qemu-forbidden.txt`.
pub(super) fn forbidden_patterns() -> Result<Vec<String>> {
    read_console_patterns(
        &workspace_root()
            .join("firmware")
            .join("scripts")
            .join("qemu-forbidden.txt"),
    )
}

/// Read console patterns from a pattern file: one fixed string per line,
/// skipping blank lines and `#` comments.
pub(super) fn read_console_patterns(path: &Path) -> Result<Vec<String>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("failed to read console pattern file '{}'", path.display()))?;
    let patterns: Vec<String> = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(String::from)
        .collect();
    if patterns.is_empty() {
        bail!(
            "console pattern file '{}' contains no patterns",
            path.display()
        );
    }
    Ok(patterns)
}

fn save_optional_output(out: Option<&Path>, output: &str) {
    if let Some(path) = out
        && let Err(err) = write_capture_file(path, output)
    {
        warn!(
            "failed to write QEMU capture to '{}': {err:#}",
            path.display()
        );
    }
}

fn write_capture_file(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| {
            format!("failed to create capture directory '{}'", parent.display())
        })?;
    }
    fs::write(path, content)
        .with_context(|| format!("failed to write capture file '{}'", path.display()))
}

fn run_once(run: &QemuRun) -> Result<Attempt> {
    let mut cmd = Command::new("qemu-system-riscv64");
    cmd.arg("-machine")
        .arg(&run.machine)
        .arg("-m")
        .arg(format!("{QEMU_MEMORY_MB}M"))
        .args(["-smp", &run.smp.to_string(), "-nographic", "-bios"])
        .arg(&run.bios);

    match &run.payload {
        QemuPayload::Payload => {}
        QemuPayload::Dynamic {
            kernel_elf,
            bootargs,
        } => {
            cmd.arg("-kernel")
                .arg(kernel_elf)
                .arg("-append")
                .arg(bootargs);
        }
        QemuPayload::Jump {
            kernel_bin,
            load_addr,
        } => {
            cmd.arg("-device").arg(format!(
                "loader,file={},addr={load_addr:#x}",
                kernel_bin.display()
            ));
        }
    }

    let mut child = cmd
        .stdin(Stdio::null())
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

enum Stream {
    Stdout,
    Stderr,
}

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

fn join_stream_reader(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    name: &str,
) -> Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("QEMU {name} reader thread panicked"))?
        .with_context(|| format!("failed to read QEMU {name}"))
}

fn print_tail(label: &str, output: &str) {
    let lines: Vec<&str> = output.lines().collect();
    let skip = lines.len().saturating_sub(LOG_TAIL_LINES);
    eprintln!(
        "----- {label} QEMU console output (last {} of {} lines) -----",
        lines.len() - skip,
        lines.len()
    );
    for line in &lines[skip..] {
        eprintln!("{line}");
    }
    eprintln!("----- end of {label} QEMU console output -----");
}
