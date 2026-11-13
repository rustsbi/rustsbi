// SPDX-License-Identifier: MulanPSL-2.0

mod plan;
mod runner;

use serde::{Deserialize, Serialize};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

pub const HELP: &str = "Usage: run.sh 'plan|run SUITE [--repo DIR] [--qemu-bin FILE] OUTPUT_DIR'
Suites: build, library, prototyper, qemu, pack, all
OUTPUT_DIR must not exist. Relative paths are resolved from the caller's directory.
Build output uses REPO/target. No packages or toolchains are installed.";

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Debug)]
struct Options {
    mode: String,
    suite: String,
    repo: PathBuf,
    output: PathBuf,
    qemu_bin: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Step {
    id: String,
    argv: Vec<String>,
    source: String,
    artifacts: Vec<PathBuf>,
    verifies_boot: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Plan {
    schema_version: u32,
    mode: String,
    suite: String,
    repo: PathBuf,
    output: PathBuf,
    toolchain: String,
    head: Option<String>,
    branch: Option<String>,
    dirty: bool,
    target_dir: PathBuf,
    qemu_bin: Option<PathBuf>,
    required_tools: Vec<String>,
    required_components: Vec<String>,
    required_targets: Vec<String>,
    steps: Vec<Step>,
}

fn absolute(path: &str, cwd: &Path) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        cwd.join(path)
    }
}

fn parse(raw: &str, cwd: &Path) -> Result<Options> {
    let tokens = shlex::split(raw).ok_or("unclosed quote in invocation")?;
    if tokens.len() < 3 || !matches!(tokens[0].as_str(), "plan" | "run") {
        return Err(HELP.into());
    }
    let suite = tokens[1].as_str();
    if !matches!(
        suite,
        "build" | "library" | "prototyper" | "qemu" | "pack" | "all"
    ) {
        return Err(format!("unknown suite: {suite}").into());
    }
    let output = tokens.last().unwrap();
    if output.starts_with("--") || output.is_empty() {
        return Err("the last argument must be a new output directory".into());
    }
    let mut repo = cwd.to_path_buf();
    let mut qemu_bin = None;
    let mut seen = std::collections::HashSet::new();
    let mut index = 2;
    while index < tokens.len() - 1 {
        let key = &tokens[index];
        if !seen.insert(key) || index + 1 >= tokens.len() - 1 {
            return Err(format!("duplicate option or missing value: {key}").into());
        }
        let value = &tokens[index + 1];
        if value.is_empty() || value.starts_with("--") {
            return Err(format!("missing value for {key}").into());
        }
        match key.as_str() {
            "--repo" => repo = absolute(value, cwd),
            "--qemu-bin" => qemu_bin = Some(absolute(value, cwd)),
            _ => return Err(format!("unknown option: {key}").into()),
        }
        index += 2;
    }
    let repo = repo.canonicalize()?;
    for required in [
        "Cargo.toml",
        "rust-toolchain.toml",
        ".cargo/config.toml",
        "firmware/prototyper/Cargo.toml",
        "library/rustsbi/Cargo.toml",
    ] {
        if !repo.join(required).is_file() {
            return Err(format!(
                "not a RustSBI workspace: missing {required} in {}",
                repo.display()
            )
            .into());
        }
    }
    if let Some(path) = &qemu_bin
        && path.file_name().and_then(|s| s.to_str()) != Some("qemu-system-riscv64")
    {
        return Err("--qemu-bin must name a qemu-system-riscv64 executable".into());
    }
    Ok(Options {
        mode: tokens[0].clone(),
        suite: suite.into(),
        repo,
        output: absolute(output, cwd),
        qemu_bin,
    })
}

fn read_toml(path: &Path) -> Result<toml::Value> {
    Ok(toml::from_str(&fs::read_to_string(path)?)?)
}

pub fn run(raw: &str) -> Result<u8> {
    if !cfg!(target_os = "linux") {
        return Err("this version supports Linux hosts only".into());
    }
    let options = parse(raw, &env::current_dir()?)?;
    let plan = plan::resolve(options)?;
    runner::execute(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_invocations_fail_before_path_resolution() {
        for raw in [
            "run build",
            "run unknown out",
            "run build --repo out",
            "run build --unknown value out",
            "run build --repo a --repo b out",
            "run build \"unterminated",
        ] {
            assert!(parse(raw, Path::new("/nonexistent")).is_err(), "{raw}");
        }
    }
}
