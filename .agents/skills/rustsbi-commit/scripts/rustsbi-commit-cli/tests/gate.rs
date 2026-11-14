// SPDX-License-Identifier: MulanPSL-2.0

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
use tempfile::TempDir;

const GOOD: &str = "fix(runtime): preserve the selected hart\n\nKeep the boot hart stable during discovery to avoid choosing an offline hart.\n\nSigned-off-by: Developer <developer@example.org>\n";
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    repo: PathBuf,
}

fn clean(command: &mut Command) -> &mut Command {
    for key in [
        "GIT_DIR",
        "GIT_WORK_TREE",
        "GIT_INDEX_FILE",
        "GIT_AUTHOR_NAME",
        "GIT_AUTHOR_EMAIL",
        "GIT_COMMITTER_NAME",
        "GIT_COMMITTER_EMAIL",
        "GIT_CONFIG_COUNT",
        "GIT_CONFIG_PARAMETERS",
    ] {
        command.env_remove(key);
    }
    command.env("GIT_CONFIG_NOSYSTEM", "1").env(
        "GIT_CONFIG_GLOBAL",
        if cfg!(windows) { "NUL" } else { "/dev/null" },
    )
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        let repo = root.join("checkout with spaces");
        fs::create_dir(&repo).unwrap();
        let fixture = Self {
            _temp: temp,
            root,
            repo,
        };
        fixture.git(&["init", "-q"]);
        fixture.git(&["config", "user.name", "Developer"]);
        fixture.git(&["config", "user.email", "developer@example.org"]);
        fixture.git(&["config", "core.autocrlf", "false"]);
        fixture.git(&["config", "core.hooksPath", "disabled-hooks"]);
        fixture.stage("source.rs", "pub fn run() {}\n");
        fixture
    }

    fn git(&self, args: &[&str]) -> Vec<u8> {
        let output = clean(Command::new("git").current_dir(&self.repo))
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    fn stage(&self, path: &str, text: &str) {
        let file = self.repo.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, text).unwrap();
        self.git(&["add", "--", path]);
    }

    fn request(&self, message: &str, paths: &[&str]) -> Value {
        fs::write(self.root.join("message.txt"), message).unwrap();
        json!({"repo": self.repo, "message_file": "message.txt", "expected_paths": paths})
    }

    fn invoke(&self, request: &Value, overrides: &[(&str, &str)]) -> (Output, PathBuf) {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let input = self.root.join(format!("request-{id}.json"));
        let output = self.root.join(format!("report-{id}.json"));
        fs::write(&input, serde_json::to_vec(request).unwrap()).unwrap();
        let mut command = Command::new(env!("CARGO_BIN_EXE_rustsbi-commit-cli"));
        clean(&mut command).arg("check").arg(input).arg(&output);
        for (key, value) in overrides {
            command.env(key, value);
        }
        (command.output().unwrap(), output)
    }

    fn check(&self, message: &str, paths: &[&str]) -> (i32, Value) {
        let request = self.request(message, paths);
        let (output, path) = self.invoke(&request, &[]);
        let report = fs::read(&path)
            .unwrap_or_else(|_| panic!("{}", String::from_utf8_lossy(&output.stderr)));
        (
            output.status.code().unwrap(),
            serde_json::from_slice(&report).unwrap(),
        )
    }
}

fn contains(report: &Value, needle: &str) -> bool {
    report["errors"]
        .as_array()
        .unwrap()
        .iter()
        .any(|error| error.as_str().unwrap().contains(needle))
}

#[test]
fn valid_check_does_not_modify_index_head_or_worktree() {
    let f = Fixture::new();
    let index = f.git(&["ls-files", "--stage", "-z"]);
    let status = f.git(&["status", "--porcelain=v1", "-z"]);
    let (code, report) = f.check(GOOD, &["source.rs"]);
    assert_eq!(code, 0);
    assert_eq!(report["changed_lines"], 1);
    assert!(report["head"].is_null());
    assert_eq!(index, f.git(&["ls-files", "--stage", "-z"]));
    assert_eq!(status, f.git(&["status", "--porcelain=v1", "-z"]));
}

#[test]
fn scope_prefix_and_human_coauthor_are_accepted() {
    let f = Fixture::new();
    let message = GOOD.replace("fix(runtime)", "rt").replace(
        "Signed-off-by:",
        "Co-authored-by: Another Human <human@example.org>\nSigned-off-by:",
    );
    assert_eq!(f.check(&message, &["source.rs"]).0, 0);
}

#[test]
fn effective_identity_overrides_cannot_hide_noreply() {
    let f = Fixture::new();
    let request = f.request(GOOD, &["source.rs"]);
    for key in ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"] {
        let (output, path) = f.invoke(&request, &[(key, "123+dev@users.noreply.github.com")]);
        assert_eq!(output.status.code(), Some(1));
        let report: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        assert!(contains(&report, "noreply"));
    }
}

#[test]
fn signoff_must_be_a_parsed_matching_footer() {
    let f = Fixture::new();
    for message in [
        GOOD.replace(
            "Developer <developer@example.org>",
            "Other <other@example.org>",
        ),
        format!("{GOOD}\nThis prose makes the former sign-off an ordinary paragraph.\n"),
    ] {
        let (code, report) = f.check(&message, &["source.rs"]);
        assert_eq!(code, 1);
        assert!(contains(&report, "Signed-off-by"));
    }
}

#[test]
fn committer_signoff_and_case_insensitive_match_follow_dco_ci() {
    let f = Fixture::new();
    let request = f.request(
        &GOOD.replace(
            "Developer <developer@example.org>",
            "Integrator <integrator@example.org>",
        ),
        &["source.rs"],
    );
    let (output, _) = f.invoke(
        &request,
        &[
            ("GIT_COMMITTER_NAME", "INTEGRATOR"),
            ("GIT_COMMITTER_EMAIL", "INTEGRATOR@example.org"),
        ],
    );
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn tool_authorship_and_generation_credits_are_blocked() {
    let f = Fixture::new();
    for attribution in [
        "Co-authored-by: Claude <claude@anthropic.com>",
        "Co-Authored-By: Codex <tool@example.org>",
        "Generated-by: a local helper",
        "Assisted-by: another model",
        "Co-authored-by: helper[bot] <helper@example.org>",
        "Co-authored-by: AI Assistant <helper@example.org>",
        "Generated with [Claude Code](https://claude.com/claude-code)",
    ] {
        let message = GOOD.replace("Signed-off-by:", &format!("{attribution}\nSigned-off-by:"));
        let (code, report) = f.check(&message, &["source.rs"]);
        assert_eq!(code, 1, "{attribution}");
        assert!(contains(&report, "attribution"));
    }
}

#[test]
fn tool_author_identity_is_blocked_even_with_human_committer_signoff() {
    let f = Fixture::new();
    let request = f.request(GOOD, &["source.rs"]);
    let (output, path) = f.invoke(&request, &[("GIT_AUTHOR_NAME", "Codex")]);
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert!(contains(&report, "tool/bot identity"));
}

#[test]
fn checked_commit_preserves_unrelated_unstaged_work() {
    let f = Fixture::new();
    fs::write(f.repo.join("unrelated.txt"), "keep this local work\n").unwrap();
    let (code, _) = f.check(GOOD, &["source.rs"]);
    assert_eq!(code, 0);
    f.git(&[
        "commit",
        "--file",
        f.root.join("message.txt").to_str().unwrap(),
        "--cleanup=verbatim",
    ]);
    let committed = f.git(&["show", "-s", "--format=%B", "HEAD"]);
    assert_eq!(
        String::from_utf8(committed).unwrap().trim_end(),
        GOOD.trim_end()
    );
    let paths = f.git(&[
        "diff-tree",
        "--root",
        "--no-commit-id",
        "--name-only",
        "-r",
        "HEAD",
    ]);
    assert_eq!(String::from_utf8(paths).unwrap().trim(), "source.rs");
    assert_eq!(
        fs::read_to_string(f.repo.join("unrelated.txt")).unwrap(),
        "keep this local work\n"
    );
}

#[test]
fn line_and_subject_width_boundaries_are_enforced() {
    let f = Fixture::new();
    let prefix_len = "fix(runtime): ".len();
    let short = GOOD.replace("preserve the selected hart", &"x".repeat(72 - prefix_len));
    assert_eq!(f.check(&short, &["source.rs"]).0, 0);
    assert!(contains(
        &f.check(
            &GOOD.replace("preserve the selected hart", &"x".repeat(73 - prefix_len)),
            &["source.rs"]
        )
        .1,
        "72"
    ));
    for (width, expected) in [(100, 0), (101, 1)] {
        let message = GOOD.replace(
            "Keep the boot hart stable during discovery to avoid choosing an offline hart.",
            &"x".repeat(width),
        );
        assert_eq!(f.check(&message, &["source.rs"]).0, expected);
    }
}

#[test]
fn malformed_header_missing_body_and_non_english_prose_block() {
    let f = Fixture::new();
    for message in [
        GOOD.replace("fix(runtime):", "Fix:"),
        GOOD.replace("selected hart\n", "selected hart.\n"),
        "fix: preserve hart\n\nSigned-off-by: Developer <developer@example.org>\n".into(),
        GOOD.replace(
            "Keep the boot hart stable during discovery to avoid choosing an offline hart.",
            "修复启动 hart。",
        ),
        GOOD.replace("\n\nKeep", "\nKeep"),
    ] {
        assert_eq!(f.check(&message, &["source.rs"]).0, 1);
    }
}

#[test]
fn unexpected_or_missing_staged_paths_block() {
    let f = Fixture::new();
    f.stage("unrelated.txt", "unrelated\n");
    assert!(contains(
        &f.check(GOOD, &["source.rs"]).1,
        "unexpected staged path"
    ));
    assert!(contains(
        &f.check(GOOD, &["source.rs", "unrelated.txt", "missing"]).1,
        "expected path is not staged"
    ));
}

#[test]
fn intended_artifact_requires_exact_documented_exception() {
    let f = Fixture::new();
    f.stage("tests/boot.log", "expected boot fixture\n");
    assert!(contains(
        &f.check(GOOD, &["source.rs", "tests/boot.log"]).1,
        "artifact"
    ));
    let mut request = f.request(GOOD, &["source.rs", "tests/boot.log"]);
    request["intentional_artifacts"] =
        json!({"tests/boot.log": "Developer requested this console regression fixture"});
    assert_eq!(f.invoke(&request, &[]).0.status.code(), Some(0));
    request["intentional_artifacts"] = json!({"tests/boot.log": ""});
    assert_eq!(f.invoke(&request, &[]).0.status.code(), Some(2));
}

#[test]
fn large_diff_boundary_requires_explicit_reason() {
    let f = Fixture::new();
    f.stage("source.rs", &"line\n".repeat(10_000));
    assert_eq!(f.check(GOOD, &["source.rs"]).0, 0);
    f.stage("source.rs", &"line\n".repeat(10_001));
    assert!(contains(&f.check(GOOD, &["source.rs"]).1, "10000"));
    let mut request = f.request(GOOD, &["source.rs"]);
    request["large_change_reason"] =
        json!("Developer explicitly requested the complete generated register table");
    assert_eq!(f.invoke(&request, &[]).0.status.code(), Some(0));
}

#[test]
fn rename_counts_deletions_plus_additions_and_both_paths() {
    let f = Fixture::new();
    f.stage("source.rs", &"line\n".repeat(5_001));
    f.git(&["commit", "-qm", "fixture baseline"]);
    f.git(&["mv", "source.rs", "moved.rs"]);
    let (code, report) = f.check(GOOD, &["source.rs", "moved.rs"]);
    assert_eq!(code, 1);
    assert_eq!(report["changed_lines"], 10_002);
    assert_eq!(report["changes"].as_array().unwrap().len(), 2);
}

#[test]
fn binary_change_is_reported_without_claiming_text_loc() {
    let f = Fixture::new();
    fs::write(f.repo.join("image.dat"), b"\0binary\xff").unwrap();
    f.git(&["add", "--", "image.dat"]);
    let (_, report) = f.check(GOOD, &["source.rs", "image.dat"]);
    let binary = report["changes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|change| change["path"] == "image.dat")
        .unwrap();
    assert!(binary["added"].is_null());
}

#[test]
fn staged_content_and_whitespace_win_over_worktree() {
    let f = Fixture::new();
    f.stage("source.rs", "bad whitespace \n");
    fs::write(f.repo.join("source.rs"), "clean unstaged\n").unwrap();
    assert!(contains(&f.check(GOOD, &["source.rs"]).1, "whitespace"));
}

#[test]
fn module_changelog_must_be_staged_and_retained() {
    let f = Fixture::new();
    f.stage("library/sbi-rt/src/lib.rs", "// change\n");
    assert!(contains(
        &f.check(GOOD, &["source.rs", "library/sbi-rt/src/lib.rs"]).1,
        "CHANGELOG"
    ));
    f.stage(
        "library/sbi-rt/CHANGELOG.md",
        "- Fix the relevant behavior.\n",
    );
    let paths = [
        "source.rs",
        "library/sbi-rt/src/lib.rs",
        "library/sbi-rt/CHANGELOG.md",
    ];
    assert_eq!(f.check(GOOD, &paths).0, 0);
    f.git(&["commit", "-qm", "fixture baseline"]);
    f.git(&["rm", "library/sbi-rt/CHANGELOG.md"]);
    assert!(contains(
        &f.check(GOOD, &["library/sbi-rt/CHANGELOG.md"]).1,
        "CHANGELOG"
    ));
}

#[test]
fn alternate_index_and_in_progress_workflows_are_refused() {
    let f = Fixture::new();
    let request = f.request(GOOD, &["source.rs"]);
    assert_eq!(
        f.invoke(&request, &[("GIT_INDEX_FILE", "another-index")])
            .0
            .status
            .code(),
        Some(2)
    );
    fs::write(f.repo.join(".git/CHERRY_PICK_HEAD"), "fixture\n").unwrap();
    assert!(contains(
        &f.check(GOOD, &["source.rs"]).1,
        "CHERRY_PICK_HEAD"
    ));
}

#[test]
fn output_collision_preserves_existing_file() {
    let f = Fixture::new();
    let request = f.request(GOOD, &["source.rs"]);
    let input = f.root.join("input.json");
    let output = f.root.join("keep.json");
    fs::write(&input, serde_json::to_vec(&request).unwrap()).unwrap();
    fs::write(&output, "user data").unwrap();
    let result = clean(&mut Command::new(env!("CARGO_BIN_EXE_rustsbi-commit-cli")))
        .args([Path::new("check"), &input, &output])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2));
    assert_eq!(fs::read_to_string(output).unwrap(), "user data");
}

#[test]
fn literal_unicode_and_tab_filenames_are_not_split() {
    let f = Fixture::new();
    let path = if cfg!(windows) {
        "space ü.txt"
    } else {
        "space\tü.txt"
    };
    f.stage(path, "literal path\n");
    assert_eq!(f.check(GOOD, &["source.rs", path]).0, 0);
}

#[test]
fn malformed_requests_and_empty_index_cannot_pass() {
    let f = Fixture::new();
    let mut request = f.request(GOOD, &["source.rs"]);
    request["allow_everything"] = json!(true);
    assert_eq!(f.invoke(&request, &[]).0.status.code(), Some(2));
    let request = f.request(GOOD, &["../source.rs"]);
    assert_eq!(f.invoke(&request, &[]).0.status.code(), Some(2));
    f.git(&["rm", "--cached", "source.rs"]);
    assert!(contains(&f.check(GOOD, &[]).1, "no staged changes"));
}
