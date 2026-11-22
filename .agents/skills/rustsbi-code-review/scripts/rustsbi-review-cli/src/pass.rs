//! run-pass subcommand — spawn one review pass and collect its comments and cost.
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

/// Read a file with UTF-8 lossy decoding; die when it is missing.
fn read_or_die(path: &std::path::Path) -> String {
    std::fs::read(path)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_else(|e| die(&format!("error: read {}: {}", path.display(), e)))
}

pub fn build_prompt(
    input_path: &str,
    personas: &[(&'static str, &'static str)],
    checks: &str,
) -> String {
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
        let suffix = rel
            .split_once("docs/src/guidelines/")
            .map(|(_, path)| path)
            .unwrap_or(rel);
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
        "# Deterministic checks".to_string(),
        "Use these results as review context. Do not duplicate findings already reported by checks."
            .to_string(),
        "```json".to_string(),
        checks.to_string(),
        "```".to_string(),
        String::new(),
        format!("Guideline root: {}", abs_root),
        "Links in an index are relative to its own persona directory.".to_string(),
        "Open a topic file only on a suspected violation, and read only that rule's section."
            .to_string(),
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

/// A `std::process::Output` stand-in so pipe and file stdio paths share code.
struct AgentOutput {
    status: std::process::ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

/// Default path: piped stdio; the prompt is far larger than a pipe buffer,
/// so feed stdin on a writer thread.
fn run_with_pipes(child: &mut std::process::Command, prompt: &str) -> AgentOutput {
    let mut child = child
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap_or_else(|e| {
            die(&format!(
                "error: cannot run `{}`: {}",
                child.get_program().to_string_lossy(),
                e
            ))
        });
    let mut stdin = child.stdin.take().unwrap();
    let prompt = prompt.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(prompt.as_bytes());
    });
    let out = child
        .wait_with_output()
        .unwrap_or_else(|e| die(&format!("error: {}", e)));
    let _ = writer.join();
    AgentOutput {
        status: out.status,
        stdout: out.stdout,
        stderr: out.stderr,
    }
}

/// Sandboxed fallback (RCR_STDIO=file): all three streams go through temp
/// files, which sidesteps pipe-based confinement of spawned agents.
fn run_with_file_stdio(child: &mut std::process::Command, prompt: &str) -> AgentOutput {
    let dir = std::env::temp_dir();
    let tag = format!("rcr-agent-{}", std::process::id());
    let in_path = dir.join(format!("{}.in", tag));
    let out_path = dir.join(format!("{}.out", tag));
    let err_path = dir.join(format!("{}.err", tag));
    std::fs::write(&in_path, prompt)
        .unwrap_or_else(|e| die(&format!("error: write {}: {}", in_path.display(), e)));
    let status = {
        let fin = std::fs::File::open(&in_path)
            .unwrap_or_else(|e| die(&format!("error: open {}: {}", in_path.display(), e)));
        let fout = std::fs::File::create(&out_path)
            .unwrap_or_else(|e| die(&format!("error: create {}: {}", out_path.display(), e)));
        let ferr = std::fs::File::create(&err_path)
            .unwrap_or_else(|e| die(&format!("error: create {}: {}", err_path.display(), e)));
        let mut child = child
            .stdin(fin)
            .stdout(fout)
            .stderr(ferr)
            .spawn()
            .unwrap_or_else(|e| {
                die(&format!(
                    "error: cannot run `{}`: {}",
                    child.get_program().to_string_lossy(),
                    e
                ))
            });
        child
            .wait()
            .unwrap_or_else(|e| die(&format!("error: {}", e)))
    };
    let stdout = std::fs::read(&out_path).unwrap_or_default();
    let stderr = std::fs::read(&err_path).unwrap_or_default();
    let _ = std::fs::remove_file(&in_path);
    let _ = std::fs::remove_file(&out_path);
    let _ = std::fs::remove_file(&err_path);
    AgentOutput {
        status,
        stdout,
        stderr,
    }
}

/// Run the agent; return (answer text, usage envelope or None).
fn run_agent(prompt: &str, agent: &str) -> (String, Option<Value>) {
    let mut cmd: Vec<String> = Vec::new();
    let mut child_env: Vec<(String, String)> = Vec::new();
    match agent {
        "claude" => {
            cmd.extend(
                [
                    "claude",
                    "-p",
                    "--output-format",
                    "json",
                    "--allowedTools",
                    "Read,Write",
                    "--permission-mode",
                    "acceptEdits",
                ]
                .iter()
                .map(|s| s.to_string()),
            );
            // Pin a model per pass with RCR_MODEL.
            if let Ok(m) = std::env::var("RCR_MODEL") {
                cmd.push("--model".to_string());
                cmd.push(m);
            }
        }
        "codex" => {
            // Codex reads the prompt from stdin when the prompt argument is "-".
            // --json emits JSONL events; usage arrives on "turn.completed".
            cmd.extend(
                [
                    "codex",
                    "exec",
                    "--json",
                    "-s",
                    "read-only",
                    "--skip-git-repo-check",
                ]
                .iter()
                .map(|s| s.to_string()),
            );
            if let Ok(m) = std::env::var("RCR_MODEL") {
                cmd.push("--model".to_string());
                cmd.push(m);
            }
            cmd.push("-".to_string());
        }
        "dsh" => {
            // dsh headless reads the prompt from stdin with "-" and answers on
            // stdout; --json emits run events whose step_end carries usage.
            // Route through the cc-switch gateway unless the caller overrode it.
            for (k, v) in [
                ("DEEPSEEK_API_KEY", "not-needed"),
                ("DEEPSEEK_BASE_URL", "http://127.0.0.1:15721"),
            ] {
                if std::env::var_os(k).is_none() {
                    child_env.push((k.to_string(), v.to_string()));
                }
            }
            let dsh = std::env::var("RCR_DSH_BIN").unwrap_or_else(|_| "dsh".to_string());
            // .cmd launchers cannot be spawned directly on Windows; go through cmd /C.
            if dsh.to_ascii_lowercase().ends_with(".cmd")
                || dsh.to_ascii_lowercase().ends_with(".bat")
            {
                cmd.push("cmd".to_string());
                cmd.push("/C".to_string());
            }
            cmd.push(dsh);
            cmd.extend(
                ["--profile", "headless", "--json"]
                    .iter()
                    .map(|s| s.to_string()),
            );
            if let Ok(p) = std::env::var("RCR_DSH_PATCH") {
                cmd.push("--patch".to_string());
                cmd.push(p);
            }
            cmd.push("-".to_string());
        }
        _ => match std::env::var("RCR_AGENT_CMD") {
            Ok(c) => cmd = c.split_whitespace().map(str::to_string).collect(),
            Err(_) => {
                die("error: no agent command; set RCR_AGENT_CMD or use --agent claude|codex|dsh")
            }
        },
    }

    let mut child = std::process::Command::new(&cmd[0]);
    child.args(&cmd[1..]).envs(child_env);

    // Pipes are the default, but some sandboxes deny file writes to processes
    // reached through piped stdio; RCR_STDIO=file routes all three streams
    // through temp files instead.
    let out = if std::env::var("RCR_STDIO").ok().as_deref() == Some("file") {
        run_with_file_stdio(&mut child, prompt)
    } else {
        run_with_pipes(&mut child, prompt)
    };
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let err = if err.trim().is_empty() {
            String::from_utf8_lossy(&out.stdout).into_owned()
        } else {
            err.trim().to_string()
        };
        die(&format!("error: {}\n{}", cmd.join(" "), err));
    }

    let text = strip_noise(&String::from_utf8_lossy(&out.stdout))
        .trim()
        .to_string();
    match agent {
        // Claude prints one JSON envelope; the answer is in "result".
        "claude" => {
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
            let result = env
                .get("result")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            (result, Some(usage))
        }
        // Codex --json prints JSONL events; the final agent_message is the
        // answer, and each turn.completed carries its usage.
        "codex" => {
            let mut answer = String::new();
            let mut usage = json!({
                "input_tokens": 0i64,
                "cache_creation_input_tokens": 0i64,
                "cache_read_input_tokens": 0i64,
                "output_tokens": 0i64,
                "reasoning_output_tokens": 0i64,
            });
            let mut num_turns = 0i64;
            for line in text.lines() {
                let ev: Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                match ev.get("type").and_then(Value::as_str) {
                    Some("item.completed") => {
                        if ev["item"]["type"] == "agent_message" {
                            if let Some(t) = ev
                                .get("item")
                                .and_then(|i| i.get("text"))
                                .and_then(Value::as_str)
                            {
                                answer = t.to_string();
                            }
                        }
                    }
                    Some("turn.completed") => {
                        num_turns += 1;
                        let u = ev.get("usage").cloned().unwrap_or(Value::Null);
                        for (dst, src) in [
                            ("input_tokens", "input_tokens"),
                            ("cache_creation_input_tokens", "cache_write_input_tokens"),
                            ("cache_read_input_tokens", "cached_input_tokens"),
                            ("output_tokens", "output_tokens"),
                            ("reasoning_output_tokens", "reasoning_output_tokens"),
                        ] {
                            let add = u.get(src).and_then(Value::as_i64).unwrap_or(0);
                            if let Some(cur) = usage.get_mut(dst).and_then(|v| v.as_i64()) {
                                usage[dst] = json!(cur + add);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if answer.is_empty() {
                return (text, None);
            }
            let envelope = json!({
                "usage": usage,
                "num_turns": num_turns,
                "session_id": Value::Null,
                "agent": "codex",
            });
            (answer, Some(envelope))
        }
        // dsh --json prints run events; "text" events are the answer chunks and
        // step_end statuses carry per-step usage.
        "dsh" => {
            let mut chunks: Vec<String> = Vec::new();
            let mut usage = json!({
                "input_tokens": 0i64,
                "cache_creation_input_tokens": 0i64,
                "cache_read_input_tokens": 0i64,
                "output_tokens": 0i64,
            });
            let mut session_id = Value::Null;
            let mut num_turns = 0i64;
            for line in text.lines() {
                let ev: Value = match serde_json::from_str(line) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                match ev.get("type").and_then(Value::as_str) {
                    Some("session") => {
                        session_id = ev.get("sessionId").cloned().unwrap_or(Value::Null);
                    }
                    Some("text") => {
                        if let Some(t) = ev.get("text").and_then(Value::as_str) {
                            chunks.push(t.to_string());
                        }
                    }
                    Some("status")
                        if ev.get("phase").and_then(Value::as_str) == Some("step_end") =>
                    {
                        num_turns += 1;
                        let u = ev.get("usage").cloned().unwrap_or(Value::Null);
                        for (dst, src) in [
                            ("input_tokens", "inputTokens"),
                            ("cache_read_input_tokens", "cacheReadTokens"),
                            ("output_tokens", "outputTokens"),
                        ] {
                            let add = u.get(src).and_then(Value::as_i64).unwrap_or(0);
                            if let Some(cur) = usage.get_mut(dst).and_then(|v| v.as_i64()) {
                                usage[dst] = json!(cur + add);
                            }
                        }
                    }
                    _ => {}
                }
            }
            if chunks.is_empty() {
                return (text, None);
            }
            let envelope = json!({
                "usage": usage,
                "num_turns": num_turns,
                "session_id": session_id,
                "agent": "dsh",
            });
            (chunks.join(""), Some(envelope))
        }
        _ => (text, None),
    }
}

fn py_num(v: Option<&Value>) -> String {
    match v {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => "None".to_string(),
    }
}

pub fn pass_main(argv: &[String]) -> i32 {
    let mut checks_path = None;
    let mut agent = "claude".to_string();
    let mut wanted: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        if argv[i] == "--checks" {
            checks_path = Some(
                argv.get(i + 1)
                    .unwrap_or_else(|| die("error: --checks requires a report path"))
                    .clone(),
            );
            i += 2;
        } else if argv[i] == "--agent" {
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
        die("usage: run-pass <review-input> <comments.json> --checks <checks.json> [--persona NAME ...]");
    }
    let (input_path, dest) = (&rest[0], &rest[1]);
    let checks_path = checks_path
        .unwrap_or_else(|| die("error: run checks first and pass --checks <checks.json>"));
    let checks = crate::checks::read_for_review(&checks_path)
        .unwrap_or_else(|error| die(&format!("error: {error:#}")));

    let personas: Vec<(&'static str, &'static str)> = PERSONAS
        .iter()
        .filter(|(n, _)| wanted.is_empty() || wanted.iter().any(|w| w.as_str() == *n))
        .copied()
        .collect();

    let prompt = build_prompt(input_path, &personas, &checks);
    let (answer, usage) = run_agent(&prompt, &agent);

    let body = match extract_json(&answer) {
        Some(b) => b,
        None => {
            let _ = std::fs::write(format!("{}.raw", dest), &answer);
            die(&format!(
                "error: the pass returned no JSON array; raw output in {}.raw",
                dest
            ));
        }
    };
    let comments: Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => {
            let _ = std::fs::write(format!("{}.raw", dest), &answer);
            die(&format!(
                "error: invalid JSON: {}; raw output in {}.raw",
                e, dest
            ));
        }
    };

    let n = comments.as_array().map(|a| a.len()).unwrap_or(0);
    std::fs::write(
        dest,
        serde_json::to_string_pretty(&comments).unwrap() + "\n",
    )
    .unwrap_or_else(|e| die(&format!("error: write {}: {}", dest, e)));

    if let Some(mut u) = usage {
        u["personas"] = json!(personas
            .iter()
            .map(|(n, _)| n.to_string())
            .collect::<Vec<_>>());
        std::fs::write(
            format!("{}.usage.json", dest),
            serde_json::to_string_pretty(&u).unwrap() + "\n",
        )
        .unwrap_or_else(|e| die(&format!("error: write usage: {}", e)));
        let tokens = &u["usage"];
        let cost = u["total_cost_usd"].as_f64().unwrap_or(0.0);
        println!(
            "{}: {} comments  |  in={} out={} cache_read={} cost=${:.4}",
            dest,
            n,
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
