// SPDX-License-Identifier: MulanPSL-2.0

use crate::Result;
use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

pub fn run(repo: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Output> {
    let mut command = Command::new("git");
    command
        .args(["--no-optional-locks", "--literal-pathspecs", "-C"])
        .arg(repo)
        .args(args)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.stdin(if input.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = command.spawn()?;
    if let Some(bytes) = input {
        child
            .stdin
            .take()
            .ok_or("missing Git stdin")?
            .write_all(bytes)?;
    }
    Ok(child.wait_with_output()?)
}

pub fn bytes(repo: &Path, args: &[&str], input: Option<&[u8]>) -> Result<Vec<u8>> {
    let output = run(repo, args, input)?;
    if !output.status.success() {
        return Err(format!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    Ok(output.stdout)
}

pub fn text(repo: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(bytes(repo, args, None)?)?
        .trim_end()
        .into())
}
