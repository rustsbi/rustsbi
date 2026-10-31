//! run-pass subcommand — spawn one review pass and collect its comments and cost.
//! Rust port of scripts/run_pass.py.
//!
//! Composes the pass prompt deterministically (pass_contract.md -> the
//! activated personas' L1 index -> the review input) and hands it to a
//! headless agent on stdin.

use crate::skillenv::{guidelines_root, skill_dir, PERSONAS};
use crate::util::die;
use regex::Regex;
use serde_json::{json, Value};
use std::io::Write;

fn noise_re() -> Regex {
    Regex::new(r"^\[claude-code:[^\]]*\]").unwrap()
}

/// Read a file with UTF-8 lossy decoding; die when it is missing (Python
/// run_pass.read raises FileNotFoundError).
fn read_or_die(path: &std::path::Path) -> String {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_else(|e| die(&format!("error: read {}: {}", path.display(), e)))
}

pub fn build_prompt(input_path: &str, personas: &[(&'static str, &'static str)]) -> String {
    let root = guidelines_root();
    let skill = skill_dir();
    let mut parts: Vec<String> = vec![
        read_or_die(&skill.join("pass_contract.md")),
        "\n".to_string(),
        "---".to_string(),
        "\n".to_string(),
    ];
    for (name, rel) in personas {
        // The table stores paths under docs/src/guidelines/; the root may be
        // overridden, so keep only the persona-relative suffix.
        let suffix = rel.splitn(2, "docs/src/guidelines/").nth(1).unwrap_or(rel);
        let index = std::path::Path::new(&root).join(suffix);
        parts.push(format!("# Guideline index: {}", name));
        parts.push("\n".to_string());
        parts.push(read_or_die(&index));
        parts.push("\n".to_string());
    }
    // os.path.abspath without symlink resolution.
    let abs_root = if std::path::Path::new(&root).is_absolute() {
        root.clone()
    } else {
        std::env::current_dir()
            .unwrap_or_default()
            .join(&root)
            .to_string_lossy()
            .into_owned()
    };
    parts.extend([
        "---".to_string(),
        format!("Guideline root: {}", abs_root),
        "Links in an index are relative to its own persona directory.".to_string(),
        "Open a topic file only on a suspected violation, and read only that rule's section.".to_string(),
        String::new(),
        "The working tree is NOT checked out at the reviewed revision.".to_string(),
        "Review the REVIEW INPUT as given; do not run git commands.".to_string(),
        String::new(),
        read_or_die(std::path::Path::new(input_path)),
    ]);
    parts.join("\n")
}

fn strip_noise(text: &str) -> String {
    let re = noise_re();
    text.lines()
        .filter(|l| !re.is_match(l))
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_json(text: &str) -> Option<&str> {
    let start = text.find('[')?;
    let end = text.rfind(']')?;
    if end < start {
        return None;
    }
    Some(&text[start..=end])
}

/// Run the agent; return (answer text, usage envelope or None).
fn run_agent(prompt: &str, agent: &str) -> (String, Option<Value>) {
    let mut cmd: Vec<String> = Vec::new();
    if agent == "claude" {
        cmd.extend(["claude", "-p", "--output-format", "json",
                    "--allowedTools", "Read,Write", "--permission-mode", "acceptEdits"]
            .iter().map(|s| s.to_string()));
        // Pin a model per pass with RCR_MODEL.
        if let Ok(m) = std::env::var("RCR_MODEL") {
            cmd.push("--model".to_string());
            cmd.push(m);
        }
    } else if let Ok(c) = std::env::var("RCR_AGENT_CMD") {
        cmd = c.split_whitespace().map(str::to_string).collect();
    } else {
        die("error: no agent command; set RCR_AGENT_CMD or use --agent claude");
    }

    let mut child = std::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| die(&format!("error: cannot run `{}`: {}", cmd[0], e)));

    // The prompt is far larger than a pipe buffer: feed stdin on a thread.
    let mut stdin = child.stdin.take().unwrap();
    let prompt = prompt.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(prompt.as_bytes());
    });

    let out = child.wait_with_output().unwrap_or_else(|e| die(&format!("error: {}", e)));
    let _ = writer.join();
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = if err.trim().is_empty() { String::from_utf8_lossy(&out.stdout).into_owned() } else { err.trim().to_string() };
        die(&format!("error: {}\n{}", cmd.join(" "), err));
    }

    let text = strip_noise(&String::from_utf8_lossy(&out.stdout)).trim().to_string();
    if agent != "claude" {
        return (text, None);
    }
    let env: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return (text, None),
    };
    let usage = json!({
        "total_cost_usd": env.get("total_cost_usd").cloned().unwrap_or(Value::Null),
        "duration_api_ms": env.get("duration_api_ms").cloned().unwrap_or(Value::Null),
        "num_turns": env.get("num_turns").cloned().unwrap_or(Value::Null),
        "session_id": env.get("session_id").cloned().unwrap_or(Value::Null),
        "usage": env.get("usage").cloned().unwrap_or(Value::Null),
        "model_usage": env.get("modelUsage").cloned().unwrap_or(Value::Null),
    });
    let result = env.get("result").and_then(Value::as_str).unwrap_or("").to_string();
    (result, Some(usage))
}

fn py_num(v: Option<&Value>) -> String {
    match v {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => "None".to_string(),
    }
}

pub fn pass_main(argv: &[String]) -> i32 {
    let mut agent = "claude".to_string();
    let mut wanted: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        if argv[i] == "--agent" {
            agent = argv[i + 1].clone();
            i += 2;
        } else if argv[i] == "--persona" {
            wanted.push(argv[i + 1].clone());
            i += 2;
        } else {
            rest.push(argv[i].clone());
            i += 1;
        }
    }
    if rest.len() != 2 {
        die("usage: run-pass <review-input> <comments.json> [--persona NAME ...]");
    }
    let (input_path, dest) = (&rest[0], &rest[1]);

    let personas: Vec<(&'static str, &'static str)> = PERSONAS
        .iter()
        .filter(|(n, _)| wanted.is_empty() || wanted.iter().any(|w| w.as_str() == *n))
        .copied()
        .collect();

    let prompt = build_prompt(input_path, &personas);
    let (answer, usage) = run_agent(&prompt, &agent);

    let body = match extract_json(&answer) {
        Some(b) => b,
        None => {
            let _ = std::fs::write(format!("{}.raw", dest), &answer);
            die(&format!("error: the pass returned no JSON array; raw output in {}.raw", dest));
        }
    };
    let comments: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::write(format!("{}.raw", dest), &answer);
            die(&format!("error: invalid JSON: {}; raw output in {}.raw", e, dest));
        }
    };

    let n = comments.as_array().map(|a| a.len()).unwrap_or(0);
    std::fs::write(&dest, serde_json::to_string_pretty(&comments).unwrap() + "\n")
        .unwrap_or_else(|e| die(&format!("error: write {}: {}", dest, e)));

    if let Some(mut u) = usage {
        u["personas"] = json!(personas.iter().map(|(n, _)| n.to_string()).collect::<Vec<_>>());
        std::fs::write(format!("{}.usage.json", dest), serde_json::to_string_pretty(&u).unwrap() + "\n")
            .unwrap_or_else(|e| die(&format!("error: write usage: {}", e)));
        let tokens = &u["usage"];
        let cost = u["total_cost_usd"].as_f64().unwrap_or(0.0);
        println!(
            "{}: {} comments  |  in={} out={} cache_read={} cost=${:.4}",
            dest, n,
            py_num(tokens.get("input_tokens")),
            py_num(tokens.get("output_tokens")),
            py_num(tokens.get("cache_read_input_tokens")),
            cost,
        );
    } else {
        println!("{}: {} comments", dest, n);
    }
    0
}
