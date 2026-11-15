use std::{fs, path::Path, process::Command};

const CLI: &str = env!("CARGO_BIN_EXE_rustsbi-review-cli");
const VALID: &str = "pub fn value() -> u32 {\n    1\n}\n";
const FIXED: &str = "pub fn value() -> u32 {\n    2\n}\n";
const INVALID: &str = "pub fn value() -> u32 {\n    \"not a number\"\n}\n";

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .current_dir(root)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success(), "{args:?}: {output:?}");
}

fn fixture(root: &Path, package: &str) {
    fs::create_dir_all(root.join("crate/src")).unwrap();
    fs::write(
        root.join("Cargo.toml"),
        "[workspace]\nmembers = [\"crate\"]\nresolver = \"2\"\n",
    )
    .unwrap();
    fs::write(
        root.join("crate/Cargo.toml"),
        format!("[package]\nname = \"{package}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"),
    )
    .unwrap();
    fs::write(root.join("crate/src/lib.rs"), VALID).unwrap();
    fs::write(root.join(".gitignore"), "/target/\n").unwrap();
    let lock = Command::new("cargo")
        .current_dir(root)
        .args(["generate-lockfile", "--offline"])
        .output()
        .unwrap();
    assert!(lock.status.success(), "{lock:?}");
    git(root, &["init", "-q"]);
    git(root, &["config", "user.name", "Test Developer"]);
    git(root, &["config", "user.email", "developer@example.org"]);
    git(root, &["add", "--", "."]);
    git(root, &["commit", "-qm", "test: initialize fixture"]);
    git(root, &["tag", "base"]);
}

fn checks(root: &Path, output: &Path, mode: &str) -> (std::process::Output, serde_json::Value) {
    let result = Command::new(CLI)
        .current_dir(root)
        .args(["checks", mode])
        .arg(output)
        .output()
        .unwrap();
    let report = fs::read(format!("{}.checks.json", output.display()))
        .unwrap_or_else(|error| panic!("{error}: {result:?}"));
    (result, serde_json::from_slice(&report).unwrap())
}

fn assert_review_blocked(root: &Path, output: &Path) {
    for command in ["run-pass", "run-fanout"] {
        let result = Command::new(CLI)
            .current_dir(root)
            .args([command, "missing-input.md", "comments.json", "--checks"])
            .arg(format!("{}.checks.json", output.display()))
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2), "{result:?}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("review blocked"));
        assert!(!root.join("comments.json").exists());
    }
}

#[test]
fn dirty_diff_cannot_validate_a_commit_with_a_local_fix() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("repo");
    fixture(&root, "xtask");
    fs::write(root.join("crate/src/lib.rs"), INVALID).unwrap();
    git(&root, &["add", "--", "crate/src/lib.rs"]);
    git(&root, &["commit", "-qm", "test: commit a type error"]);

    let (result, report) = checks(&root, &dir.path().join("broken"), "diff base review.md");
    assert_eq!(result.status.code(), Some(1), "{result:?}");
    assert_eq!(report["review_allowed"], false);

    for state in ["unstaged", "staged", "untracked"] {
        git(&root, &["reset", "--hard", "HEAD"]);
        if state == "untracked" {
            fs::write(root.join("notes.txt"), "untracked input\n").unwrap();
        } else {
            fs::write(root.join("crate/src/lib.rs"), FIXED).unwrap();
            let compiled = Command::new("cargo")
                .current_dir(&root)
                .args(["check", "--locked", "--offline", "-p", "xtask"])
                .output()
                .unwrap();
            assert!(compiled.status.success(), "{compiled:?}");
            if state == "staged" {
                git(&root, &["add", "--", "crate/src/lib.rs"]);
            }
        }
        let output = dir.path().join(state);
        let (result, report) = checks(&root, &output, "diff base review.md");
        assert_eq!(result.status.code(), Some(1), "{state}: {result:?}");
        assert_eq!(report["overall"], "blocked", "{report}");
        assert_eq!(report["review_allowed"], false);
        assert_eq!(report["results"][0]["id"], "dirty-diff");
        assert_eq!(report["results"].as_array().unwrap().len(), 1);
        assert_review_blocked(&root, &output);
        for meta in [false, true] {
            let mut resolve = Command::new(CLI);
            resolve.current_dir(&root).arg("resolve-target");
            if meta {
                resolve.arg("--meta");
            }
            let resolved = resolve.arg("diff base review.md").output().unwrap();
            assert_eq!(resolved.status.code(), Some(2), "{resolved:?}");
            assert!(String::from_utf8_lossy(&resolved.stderr).contains("clean working tree"));
        }
        assert!(!root.join("review.md.input.md").exists());
    }

    fs::remove_file(root.join("notes.txt")).unwrap();
    fs::write(root.join("crate/src/lib.rs"), FIXED).unwrap();
    let (result, report) = checks(
        &root,
        &dir.path().join("files"),
        "files crate/src/lib.rs review.md",
    );
    assert!(result.status.success(), "{result:?}");
    assert_eq!(report["review_allowed"], true);
    git(&root, &["add", "--", "crate/src/lib.rs"]);
    git(&root, &["commit", "-qm", "test: fix the type error"]);
    let (result, report) = checks(&root, &dir.path().join("clean"), "diff base review.md");
    assert!(result.status.success(), "{result:?}");
    assert_eq!(report["review_allowed"], true);
    let resolved = Command::new(CLI)
        .current_dir(&root)
        .args(["resolve-target", "--meta", "diff base review.md"])
        .output()
        .unwrap();
    assert!(resolved.status.success(), "{resolved:?}");
}
