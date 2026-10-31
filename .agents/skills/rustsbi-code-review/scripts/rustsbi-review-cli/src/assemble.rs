//! assemble subcommand — deterministic merge of one pass's comments into the
//! review file. Rust port of scripts/assemble_review.py.
//!
//! No judgement here: group by persona in a fixed order, sort by file then
//! line, drop exact duplicates, write the frontmatter, leave the SUMMARY.

use crate::util::die;
use regex::Regex;
use serde_json::Value;

const FENCE: &str = "```";

fn kebab_re() -> Regex {
    Regex::new(r"^[a-z0-9]+(-[a-z0-9]+)*$").unwrap()
}

/// Order of persona sections in the review file.
const ORDER: [(&str, &str); 3] = [
    ("maintainability", "Maintainability"),
    ("development", "Correctness & Safety"),
    ("documentation", "Documentation"),
];

fn grounding_tag(g: Option<&str>) -> String {
    let g = g.unwrap_or("");
    if kebab_re().is_match(g) {
        format!("`{}`", g)
    } else {
        g.to_string()
    }
}

fn load_meta(path: &str) -> std::collections::HashMap<String, String> {
    let mut meta = std::collections::HashMap::new();
    let text = std::fs::read_to_string(path).unwrap_or_default();
    for line in text.lines() {
        let line = line.trim();
        if let Some((k, v)) = line.split_once('=') {
            meta.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    meta
}

fn line_of(c: &Value) -> String {
    match c.get("line") {
        Some(Value::Number(n)) => n.to_string(),
        Some(Value::String(s)) => s.clone(),
        _ => String::new(),
    }
}

fn render_comment(c: &Value) -> String {
    let file = c["file"].as_str().unwrap_or("").trim();
    let line = line_of(c);
    let mut out = Vec::new();
    if c.get("line").map(|v| v.is_null()).unwrap_or(true) {
        out.push(format!("### `{}`", file));
    } else {
        out.push(format!("### `{}` line {}", file, line));
    }
    out.push(String::new());
    let snippet = c["diff"].as_str().unwrap_or("").trim_end();
    if !snippet.is_empty() {
        out.push(format!("> {}diff", FENCE));
        for ln in snippet.lines() {
            out.push(format!("> {}", ln));
        }
        out.push(format!("> {}", FENCE));
        out.push(String::new());
    }
    let gist = c["rule_gist"].as_str().unwrap_or("").trim();
    let mut header = format!("{} ({})", grounding_tag(c["grounding"].as_str()),
                             c["severity"].as_str().unwrap_or("minor"));
    if !gist.is_empty() {
        // The gist may itself contain backticks, so it is emphasised rather
        // than wrapped in a code span, which would not nest.
        header += &format!(" — *{}*", gist);
    }
    out.push(format!("{}: {}", header, c["problem"].as_str().unwrap_or("").trim()));
    out.push(String::new());
    out.push(format!("**Fix.** {}", c["fix"].as_str().unwrap_or("").trim()));
    out.push(String::new());
    out.join("\n")
}

pub fn assemble_main(argv: &[String]) -> i32 {
    if argv.len() != 3 {
        die("usage: assemble <meta-file> <comments.json> <output-file>");
    }
    let (meta_path, frag_path, out_path) = (&argv[0], &argv[1], &argv[2]);
    if std::path::Path::new(out_path).exists() {
        die(&format!("error: refusing to overwrite {}", out_path));
    }

    let meta = load_meta(meta_path);
    let mut raw = std::fs::read_to_string(frag_path)
        .unwrap_or_else(|e| die(&format!("error: read {}: {}", frag_path, e)))
        .trim()
        .to_string();
    if raw.starts_with(FENCE) {
        // Strip the surrounding code fence the agent may have added.
        raw = raw.splitn(2, '\n').nth(1).unwrap_or("").to_string();
        if let Some(pos) = raw.rfind(FENCE) {
            raw.truncate(pos);
        }
    }
    let parsed: Value = if raw.trim().is_empty() {
        Value::Array(vec![])
    } else {
        serde_json::from_str(&raw)
            .unwrap_or_else(|e| die(&format!("error: invalid JSON: {}", e)))
    };
    let comments: Vec<Value> = match parsed {
        Value::Array(a) => a,
        Value::Object(o) => o.get("comments").and_then(Value::as_array).cloned().unwrap_or_default(),
        _ => Vec::new(),
    };

    let mut lines = vec!["---".to_string()];
    for key in ["date", "mode", "base", "files", "head", "branch", "title"] {
        if let Some(v) = meta.get(key) {
            if !v.is_empty() {
                lines.push(format!("{}: {}", key, v));
            }
        }
    }
    lines.extend(["---".to_string(), String::new(), "<!-- SUMMARY -->".to_string(), String::new()]);

    let mut kept = 0;
    for (persona, section) in ORDER {
        let mut group: Vec<&Value> = comments
            .iter()
            .filter(|c| c["persona"].as_str() == Some(persona))
            .collect();
        // Drop exact duplicates on (file, line, grounding, problem).
        let mut seen: Vec<String> = Vec::new();
        group.retain(|c| {
            let key = format!("[file={},line={},grounding={},problem={}]",
                c.get("file").cloned().unwrap_or(Value::Null),
                c.get("line").cloned().unwrap_or(Value::Null),
                c.get("grounding").cloned().unwrap_or(Value::Null),
                c.get("problem").cloned().unwrap_or(Value::Null));
            if seen.contains(&key) {
                false
            } else {
                seen.push(key);
                true
            }
        });
        group.sort_by(|a, b| {
            let fa = a["file"].as_str().unwrap_or("");
            let fb = b["file"].as_str().unwrap_or("");
            let la = a["line"].as_i64().unwrap_or(0);
            let lb = b["line"].as_i64().unwrap_or(0);
            (fa, la).cmp(&(fb, lb))
        });
        if group.is_empty() {
            continue;
        }
        lines.push(format!("## {}", section));
        lines.push(String::new());
        for c in &group {
            lines.push(render_comment(c));
        }
        kept += group.len();
    }
    let total_in = comments.len();
    while lines.last().map(|l| l.is_empty()).unwrap_or(false) {
        lines.pop();
    }
    std::fs::write(out_path, lines.join("\n") + "\n")
        .unwrap_or_else(|e| die(&format!("error: write {}: {}", out_path, e)));
    println!("{}: {} comments ({} in, {} dropped as duplicates)",
        out_path, kept, total_in, total_in - kept);
    0
}
