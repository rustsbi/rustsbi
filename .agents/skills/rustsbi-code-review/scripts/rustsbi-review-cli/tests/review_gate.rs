use std::process::Command;

const CLI: &str = env!("CARGO_BIN_EXE_rustsbi-review-cli");

#[test]
fn review_commands_reject_missing_or_blocked_checks_before_reading_input() {
    let dir = tempfile::tempdir().unwrap();
    let report = dir.path().join("blocked.checks.json");
    std::fs::write(
        &report,
        r#"{"schema":1,"review_allowed":false,"blocking_reason":"compile failure"}"#,
    )
    .unwrap();

    for command in ["run-pass", "run-fanout"] {
        for with_report in [false, true] {
            let mut process = Command::new(CLI);
            process
                .current_dir(dir.path())
                .args([command, "missing-input.md", "comments.json"]);
            if with_report {
                process.arg("--checks").arg(&report);
            }
            let output = process.output().unwrap();
            assert_eq!(output.status.code(), Some(2));
            let error = String::from_utf8_lossy(&output.stderr);
            assert!(error.contains("checks"), "{error}");
            assert!(
                !error.contains("missing-input"),
                "input was read before the gate: {error}"
            );
            assert!(!dir.path().join("comments.json").exists());
        }
    }
}

#[cfg(unix)]
#[test]
fn combined_review_pass_receives_advisory_results() {
    use std::os::unix::fs::PermissionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    std::fs::write(
        root.join("pass_contract.md"),
        "Return a JSON comment array.\n",
    )
    .unwrap();
    for persona in ["maintainability", "development", "documentation"] {
        let path = root.join(format!("guidelines/for-{persona}"));
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("README.md"), "Review the changed code.\n").unwrap();
    }
    std::fs::write(root.join("input.md"), "# REVIEW INPUT\nmode: files\n").unwrap();
    std::fs::write(
        root.join("checks.json"),
        r#"{"schema":1,"review_allowed":true,"overall":"failed","results":[{"id":"clippy-example","status":"fail","gate":"advisory","message":"example advisory finding"}]}"#,
    )
    .unwrap();
    let agent = root.join("claude");
    std::fs::write(
        &agent,
        "#!/bin/sh\ncat > \"$PROMPT_CAPTURE\"\nprintf '%s\\n' '{\"result\":\"[]\"}'\n",
    )
    .unwrap();
    std::fs::set_permissions(&agent, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = std::env::join_paths(std::iter::once(root.to_path_buf()).chain(
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()),
    ))
    .unwrap();

    let output = Command::new(CLI)
        .current_dir(root)
        .args([
            "run-fanout",
            "input.md",
            "comments.json",
            "--checks",
            "checks.json",
        ])
        .env("PATH", path)
        .env("RCR_SKILL_HOME", root)
        .env("RCR_GUIDELINES_ROOT", root.join("guidelines"))
        .env("PROMPT_CAPTURE", root.join("prompt.txt"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let prompt = std::fs::read_to_string(root.join("prompt.txt")).unwrap();
    assert!(prompt.contains("clippy-example"));
    assert!(prompt.contains("example advisory finding"));
    let comments: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("comments.json")).unwrap()).unwrap();
    assert_eq!(comments, serde_json::json!([]));
}
