// SPDX-License-Identifier: MulanPSL-2.0

use crate::{Plan, Result};
use fs2::FileExt;
use serde::Serialize;
use std::{
    env,
    fs::{self, File, OpenOptions},
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Serialize)]
struct StepResult {
    id: String,
    status: String,
    exit_code: Option<i32>,
    seconds: f64,
    log: PathBuf,
    error: Option<String>,
}

#[derive(Serialize)]
struct Report<'a> {
    plan: &'a Plan,
    status: &'a str,
    started_unix_secs: u64,
    steps: &'a [StepResult],
    error: Option<&'a str>,
    boot_verified: bool,
}

fn executable(name: &str, search_path: &std::ffi::OsStr) -> bool {
    env::split_paths(search_path).any(|dir| {
        fs::metadata(dir.join(name))
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
}

fn search_path(plan: &Plan) -> Result<std::ffi::OsString> {
    let mut directories = Vec::new();
    if let Some(binary) = &plan.qemu_bin {
        directories.push(binary.parent().unwrap().to_path_buf());
    }
    directories.extend(env::split_paths(&env::var_os("PATH").unwrap_or_default()));
    Ok(env::join_paths(directories)?)
}

fn command(plan: &Plan, argv: &[String]) -> Result<Command> {
    let mut command = Command::new(&argv[0]);
    command
        .args(&argv[1..])
        .current_dir(&plan.repo)
        .env("PATH", search_path(plan)?)
        .env("RUSTUP_AUTO_INSTALL", "0")
        .env("RUSTUP_TOOLCHAIN", &plan.toolchain)
        .env("CARGO_TARGET_DIR", &plan.target_dir)
        .env("CARGO_TERM_COLOR", "never");
    Ok(command)
}

fn query(plan: &Plan, args: &[&str]) -> Result<String> {
    let output = command(
        plan,
        &args.iter().map(|arg| (*arg).into()).collect::<Vec<_>>(),
    )?
    .output()?;
    if !output.status.success() {
        return Err(format!(
            "{}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn prerequisites(plan: &Plan) -> Result<String> {
    if let Some(binary) = &plan.qemu_bin
        && !fs::metadata(binary)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    {
        return Err(format!("selected QEMU is not executable: {}", binary.display()).into());
    }
    let path = search_path(plan)?;
    let missing: Vec<_> = plan
        .required_tools
        .iter()
        .filter(|tool| !executable(tool, &path))
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "missing host commands: {}; prepare the environment separately",
            missing
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
        .into());
    }
    let version = query(plan, &["rustc", "-vV"])?;
    let host = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .ok_or("rustc did not report its host")?
        .to_string();
    let components = query(
        plan,
        &[
            "rustup",
            "component",
            "list",
            "--installed",
            "--toolchain",
            &plan.toolchain,
        ],
    )?;
    for component in &plan.required_components {
        // The toolchain file accepts a preview alias; rustup's installed
        // inventory can report the same component without that suffix.
        let installed_name = component.strip_suffix("-preview").unwrap_or(component);
        if !components
            .lines()
            .any(|line| line == installed_name || line.starts_with(&format!("{installed_name}-")))
        {
            return Err(format!(
                "toolchain {} lacks component {component}; prepare it separately",
                plan.toolchain
            )
            .into());
        }
    }
    let targets = query(
        plan,
        &[
            "rustup",
            "target",
            "list",
            "--installed",
            "--toolchain",
            &plan.toolchain,
        ],
    )?;
    for target in &plan.required_targets {
        if !targets.lines().any(|line| line == target) {
            return Err(format!(
                "toolchain {} lacks target {target}; prepare it separately",
                plan.toolchain
            )
            .into());
        }
    }
    if plan
        .required_tools
        .iter()
        .any(|tool| tool == "qemu-system-riscv64")
    {
        let version = query(plan, &["qemu-system-riscv64", "--version"])?;
        let number = version
            .split_whitespace()
            .nth(3)
            .ok_or("unrecognized QEMU version")?;
        let numbers = number
            .split('.')
            .take(2)
            .map(str::parse::<u32>)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if numbers.len() != 2 || (numbers[0], numbers[1]) < (9, 1) {
            return Err("QEMU >= 9.1 is required by Prototyper CI".into());
        }
    }
    Ok(host)
}

fn persist(
    plan: &Plan,
    status: &str,
    started: u64,
    steps: &[StepResult],
    error: Option<&str>,
) -> Result<()> {
    let boot_steps: Vec<_> = plan
        .steps
        .iter()
        .zip(steps)
        .filter(|(step, _)| step.verifies_boot)
        .collect();
    let boot_verified = !boot_steps.is_empty()
        && boot_steps
            .iter()
            .all(|(_, result)| result.status == "passed");
    let report = Report {
        plan,
        status,
        started_unix_secs: started,
        steps,
        error,
        boot_verified,
    };
    let temporary = plan.output.join("report.json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(&report)?)?;
    fs::rename(temporary, plan.output.join("report.json"))?;
    let mut markdown = format!(
        "# RustSBI local build\n\nStatus: **{status}**\n\nSuite: `{}`  \nRepository: `{}`  \nToolchain: `{}`  \nBoot verified: `{boot_verified}`\n\n",
        plan.suite,
        plan.repo.display(),
        plan.toolchain
    );
    if let Some(error) = error {
        markdown.push_str(&format!("{error}\n\n"));
    }
    markdown.push_str("| Step | Status | Log |\n|---|---|---|\n");
    for result in steps {
        markdown.push_str(&format!(
            "| {} | {} | `{}` |\n",
            result.id,
            result.status,
            result.log.display()
        ));
    }
    fs::write(plan.output.join("report.md"), markdown)?;
    Ok(())
}

fn usage(plan: &Plan, status: &str, started: u64, error: Option<&str>) -> Result<()> {
    let directory = if let Some(path) = env::var_os("RDB_USAGE_DIR") {
        PathBuf::from(path)
    } else {
        let skill = env::var_os("RDB_SKILL_HOME")
            .map(PathBuf::from)
            .or_else(|| {
                env::current_exe()
                    .ok()?
                    .ancestors()
                    .find(|path| path.join("SKILL.md").is_file())
                    .map(Path::to_owned)
            })
            .ok_or("set RDB_USAGE_DIR or RDB_SKILL_HOME when using a detached binary")?;
        skill.join("usage")
    };
    fs::create_dir_all(&directory)?;
    let entry = serde_json::json!({"id": format!("{}-{}", started, std::process::id()),
        "timestamp_unix_secs": started, "trigger": env::var("RDB_TRIGGER").unwrap_or_else(|_| "local".into()),
        "mode": plan.mode, "suite": plan.suite, "output": plan.output, "head": plan.head,
        "branch": plan.branch, "status": status, "failure": error,
        "agent_profile": env::var("RDB_AGENT_PROFILE").ok(), "model": env::var("RDB_MODEL").ok()});
    let mut record = serde_json::to_vec(&entry)?;
    record.push(b'\n');
    OpenOptions::new()
        .create(true)
        .append(true)
        .open(directory.join("USAGE.log"))?
        .write_all(&record)?;
    Ok(())
}

pub(super) fn execute(plan: Plan) -> Result<u8> {
    // create_dir is the final authority: it refuses existing directories and symlinks.
    fs::create_dir_all(plan.output.parent().ok_or("output has no parent")?)?;
    fs::create_dir(&plan.output)?;
    fs::create_dir(plan.output.join("logs"))?;
    fs::write(
        plan.output.join("plan.json"),
        serde_json::to_vec_pretty(&plan)?,
    )?;
    let started = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let mut results: Vec<_> = plan
        .steps
        .iter()
        .enumerate()
        .map(|(index, step)| StepResult {
            id: step.id.clone(),
            status: "not-run".into(),
            exit_code: None,
            seconds: 0.0,
            log: plan
                .output
                .join("logs")
                .join(format!("{:02}-{}.log", index + 1, step.id)),
            error: None,
        })
        .collect();
    persist(&plan, "planned", started, &results, None)?;
    println!(
        "{} {}: {} steps\nReport: {}",
        plan.mode,
        plan.suite,
        plan.steps.len(),
        plan.output.display()
    );
    for step in &plan.steps {
        println!("  {}: {:?}", step.id, step.argv);
    }
    if plan.mode == "plan" {
        usage(&plan, "planned", started, None)?;
        return Ok(0);
    }
    fs::create_dir_all(&plan.target_dir)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(plan.target_dir.join(".rustsbi-dev-build.lock"))?;
    if lock.try_lock_exclusive().is_err() {
        let error = "another rustsbi-dev-build run is using this checkout";
        persist(&plan, "blocked", started, &results, Some(error))?;
        usage(&plan, "blocked", started, Some(error))?;
        eprintln!("BLOCKED: {error}");
        return Ok(3);
    }
    let host = match prerequisites(&plan) {
        Ok(host) => host,
        Err(error) => {
            let error = error.to_string();
            persist(&plan, "blocked", started, &results, Some(&error))?;
            usage(&plan, "blocked", started, Some(&error))?;
            eprintln!("BLOCKED: {error}");
            return Ok(3);
        }
    };
    for (index, step) in plan.steps.iter().enumerate() {
        println!("[{}/{}] {}", index + 1, plan.steps.len(), step.id);
        results[index].status = "running".into();
        persist(&plan, "running", started, &results, None)?;
        let start = Instant::now();
        let file = File::create(&results[index].log)?;
        let outcome = command(&plan, &step.argv)?
            .env("CARGO_BUILD_TARGET", &host)
            .stdin(Stdio::null())
            .stdout(Stdio::from(file.try_clone()?))
            .stderr(Stdio::from(file))
            .status();
        results[index].seconds = start.elapsed().as_secs_f64();
        let error = match outcome {
            Err(error) => Some(format!("could not start {}: {error}", step.id)),
            Ok(status) => {
                results[index].exit_code = status.code();
                if !status.success() {
                    Some(format!("{} failed with {status}", step.id))
                } else {
                    step.artifacts
                        .iter()
                        .find(|path| {
                            !fs::metadata(plan.repo.join(path))
                                .is_ok_and(|m| m.is_file() && m.len() > 0)
                        })
                        .map(|path| {
                            format!(
                                "{} succeeded but required artifact is absent or empty: {}",
                                step.id,
                                path.display()
                            )
                        })
                }
            }
        };
        if let Some(error) = error {
            results[index].status = "failed".into();
            results[index].error = Some(error.clone());
            persist(&plan, "failed", started, &results, Some(&error))?;
            usage(&plan, "failed", started, Some(&error))?;
            eprintln!("FAILED: {error}\nLog: {}", results[index].log.display());
            return Ok(1);
        }
        results[index].status = "passed".into();
        persist(&plan, "running", started, &results, None)?;
    }
    persist(&plan, "passed", started, &results, None)?;
    usage(&plan, "passed", started, None)?;
    println!(
        "PASSED: {}\nReport: {}",
        plan.suite,
        plan.output.join("report.md").display()
    );
    Ok(0)
}
