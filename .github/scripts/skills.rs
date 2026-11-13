// SPDX-License-Identifier: MulanPSL-2.0
// Dependency-free bootstrap for the standalone skill workspaces.

use std::{
    collections::BTreeSet,
    env,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

fn output(command: &mut Command) -> String {
    let result = command.output().expect("execute command");
    assert!(
        result.status.success(),
        "{command:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).expect("UTF-8 output")
}

fn workspaces() -> BTreeSet<String> {
    let root = env::current_dir().unwrap().canonicalize().unwrap();
    let paths = output(Command::new("git").args([
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
        ".agents/skills/**/Cargo.toml",
    ]));
    let mut projects = BTreeSet::new();
    for path in paths.split('\0').filter(|p| !p.is_empty()) {
        if !Path::new(path).is_file() {
            continue;
        }
        let manifest = output(Command::new("cargo").args([
            "locate-project",
            "--workspace",
            "--manifest-path",
            path,
            "--message-format",
            "plain",
        ]));
        let directory = Path::new(manifest.trim())
            .canonicalize()
            .unwrap()
            .parent()
            .unwrap()
            .strip_prefix(&root)
            .unwrap()
            .to_owned();
        assert!(
            directory.starts_with(".agents/skills"),
            "workspace escapes skills: {path}"
        );
        let path = directory.to_str().unwrap().replace('\\', "/");
        assert!(
            path.chars()
                .all(|c| c.is_ascii_alphanumeric() || "/-_.".contains(c)),
            "unexpected workspace path"
        );
        projects.insert(path);
    }
    projects
}

fn package_checks() {
    for name in [
        "rustsbi-qemu-dev-setup",
        "rustsbi-dev-build",
        "rustsbi-commit",
        "rustsbi-code-review",
    ] {
        let dir = PathBuf::from(".agents/skills").join(name);
        let text = fs::read_to_string(dir.join("SKILL.md")).unwrap();
        assert!(
            text.starts_with("---\n"),
            "{name}: frontmatter/BOM/line ending"
        );
        let header = text.split("---").nth(1).unwrap();
        assert!(
            header.lines().any(|l| l == format!("name: {name}")),
            "{name}: name mismatch"
        );
        assert!(
            header
                .lines()
                .any(|l| l.starts_with("description: ") && l.len() > 30),
            "{name}: missing description"
        );
        let link = PathBuf::from(".claude/skills").join(name);
        let expected = format!("../../.agents/skills/{name}");
        let actual = fs::read_link(&link)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| fs::read_to_string(link).unwrap());
        assert_eq!(actual, expected, "{name}: cross-tool link");
    }
    let paths = output(Command::new("git").args([
        "ls-files",
        "--cached",
        "--others",
        "--exclude-standard",
        "-z",
        "--",
        ".agents/skills",
    ]));
    for path in paths
        .split('\0')
        .filter(|p| !p.is_empty() && Path::new(p).is_file())
    {
        assert!(
            !path.ends_with(".py"),
            "unexpected interpreter script: {path}"
        );
        if path.ends_with(".sh") {
            assert!(
                Command::new("bash")
                    .args(["-n", path])
                    .status()
                    .unwrap()
                    .success(),
                "shell syntax: {path}"
            );
        }
        if path.ends_with(".md")
            || path.ends_with(".rs")
            || path.ends_with(".sh")
            || path.ends_with(".toml")
        {
            let text = fs::read_to_string(path).unwrap();
            assert!(
                !text.contains('\r') && text.ends_with('\n') && !text.ends_with("\n\n"),
                "line endings or trailing blank lines: {path}"
            );
        }
    }
    println!("Skill package metadata, links, shell syntax and text endings passed");
}

fn main() {
    match env::args().nth(1).as_deref() {
        Some("discover") => {
            let projects: Vec<_> = workspaces().into_iter().collect();
            let result = format!(
                "matrix={{\"project\":{projects:?}}}\nhas-workspaces={}\n",
                !projects.is_empty()
            );
            print!("{result}");
            if let Ok(path) = env::var("GITHUB_OUTPUT") {
                OpenOptions::new()
                    .append(true)
                    .open(path)
                    .unwrap()
                    .write_all(result.as_bytes())
                    .unwrap();
            }
        }
        Some("check") => package_checks(),
        _ => panic!("usage: skills discover|check"),
    }
}
