//! run-fanout subcommand — run a review in one of two modes.
//! Rust port of scripts/run_fanout.py.
//!
//! combined (default): one pass, every activated persona index in context.
//! persona: one pass per persona (selective exposure), run in parallel.
//! Passes run as child `run-pass` processes; the merged list is
//! de-duplicated and token usage summed into <comments.json>.usage.json.

use crate::util::die;
use serde_json::{json, Value};

const DEFAULT: [&str; 3] = ["maintainability", "development", "documentation"];

struct PassOutcome {
    dest: String,
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

fn spawn_pass(src: &str, dest: &str, personas: &[String]) -> std::thread::JoinHandle<PassOutcome> {
    let src = src.to_string();
    let dest = dest.to_string();
    let personas = personas.to_vec();
    std::thread::spawn(move || {
        let exe = std::env::current_exe().unwrap_or_else(|_| "rustsbi-review-cli".into());
        let mut cmd = std::process::Command::new(&exe);
        cmd.arg("run-pass").arg(&src).arg(&dest);
        for p in &personas {
            cmd.arg("--persona").arg(p);
        }
        cmd.stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .stdin(std::process::Stdio::null());
        match cmd.output() {
            Ok(out) => PassOutcome {
                dest,
                stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
                stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
                code: out.status.code(),
            },
            Err(e) => PassOutcome {
                dest,
                stdout: String::new(),
                stderr: format!("spawn run-pass: {e}"),
                code: Some(-1),
            },
        }
    })
}

pub fn fanout_main(argv: &[String]) -> i32 {
    let mut mode = "combined".to_string();
    let mut personas: Vec<String> = Vec::new();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        if argv[i] == "--mode" {
            mode = argv[i + 1].clone();
            i += 2;
        } else if argv[i] == "--persona" {
            personas.push(argv[i + 1].clone());
            i += 2;
        } else {
            rest.push(argv[i].clone());
            i += 1;
        }
    }
    if rest.len() != 2 {
        die("usage: run-fanout <review-input> <comments.json> [--mode combined|persona] [--persona NAME ...]");
    }
    if mode != "combined" && mode != "persona" {
        die("error: --mode must be combined or persona");
    }
    let (src, out) = (&rest[0], &rest[1]);
    let personas: Vec<String> = if personas.is_empty() {
        DEFAULT.iter().map(|s| s.to_string()).collect()
    } else {
        personas
    };

    // (label, dest, personas-for-this-pass)
    let mut jobs: Vec<(String, String, Vec<String>)> = Vec::new();
    if mode == "combined" {
        let dest = format!("{}.combined.json", out);
        jobs.push((personas.join(","), dest, personas.clone()));
    } else {
        for name in &personas {
            let dest = format!("{}.{}.json", out, name);
            jobs.push((name.clone(), dest, vec![name.clone()]));
        }
    }

    let handles: Vec<_> = jobs
        .iter()
        .map(|(_, dest, ps)| spawn_pass(src, dest, ps))
        .collect();

    let mut merged: Vec<Value> = Vec::new();
    let mut failed: Vec<(String, String)> = Vec::new();
    let mut passes: Vec<Value> = Vec::new();
    let mut totals = json!({
        "cache_creation_input_tokens": 0,
        "cache_read_input_tokens": 0,
        "input_tokens": 0,
        "output_tokens": 0,
    });

    for ((name, _dest, _), h) in jobs.iter().zip(handles) {
        let o = h.join().unwrap_or_else(|_| die("error: run-pass thread panicked"));
        let ok = o.code == Some(0) && std::path::Path::new(&o.dest).exists();
        if !ok {
            let msg = if !o.stderr.trim().is_empty() { o.stderr.trim() } else { o.stdout.trim() };
            let tail: String = msg.chars().rev().take(300).collect::<Vec<_>>().into_iter().rev().collect();
            failed.push((name.clone(), tail));
            continue;
        }
        let got: Value = std::fs::read_to_string(&o.dest)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(Value::Array(vec![]));
        let n = got.as_array().map(|a| a.len()).unwrap_or(0);
        println!("  {}: {} comments", name, n);
        if let Some(arr) = got.as_array().cloned() {
            merged.extend(arr);
        }
        let upath = format!("{}.usage.json", o.dest);
        if let Ok(text) = std::fs::read_to_string(&upath) {
            if let Ok(u) = serde_json::from_str::<Value>(&text) {
                passes.push(json!({
                    "persona": name,
                    "usage": u.get("usage").cloned().unwrap_or(Value::Null),
                    "duration_api_ms": u.get("duration_api_ms").cloned().unwrap_or(Value::Null),
                    "num_turns": u.get("num_turns").cloned().unwrap_or(Value::Null),
                }));
                for k in ["cache_creation_input_tokens", "cache_read_input_tokens", "input_tokens", "output_tokens"] {
                    let add = u["usage"].get(k).and_then(Value::as_i64).unwrap_or(0);
                    if let Some(cur) = totals.get_mut(k).and_then(|v| v.as_i64()) {
                        totals[k] = json!(cur + add);
                    }
                }
            }
        }
    }

    for (name, err) in &failed {
        eprintln!("pass {} failed: {}", name, err);
    }

    // De-duplicate on (file, line, grounding, problem[:80]).
    let mut seen: Vec<(Value, Value, Value, String)> = Vec::new();
    let mut unique: Vec<Value> = Vec::new();
    for c in &merged {
        let problem = c["problem"].as_str().unwrap_or("");
        let head: String = problem.chars().take(80).collect();
        let key = (c.get("file").cloned().unwrap_or(Value::Null),
                   c.get("line").cloned().unwrap_or(Value::Null),
                   c.get("grounding").cloned().unwrap_or(Value::Null),
                   head);
        if !seen.contains(&key) {
            seen.push(key);
            unique.push(c.clone());
        }
    }

    std::fs::write(&out, serde_json::to_string_pretty(&Value::Array(unique.clone())).unwrap() + "\n")
        .unwrap_or_else(|e| die(&format!("error: write {}: {}", out, e)));

    if !passes.is_empty() {
        let sidecar = json!({"mode": mode, "passes": passes, "totals": totals});
        std::fs::write(format!("{}.usage.json", out), serde_json::to_string_pretty(&sidecar).unwrap() + "\n")
            .unwrap_or_else(|e| die(&format!("error: write usage: {}", e)));
        println!("tokens: cache_wr={} cache_rd={} plain_rd={} write={}",
            totals["cache_creation_input_tokens"], totals["cache_read_input_tokens"],
            totals["input_tokens"], totals["output_tokens"]);
    }

    println!("{}: {} comments from {} pass(es), mode={} ({} duplicates merged)",
        out, unique.len(), jobs.len(), mode, merged.len() - unique.len());
    if !failed.is_empty() {
        return 1;
    }
    0
}