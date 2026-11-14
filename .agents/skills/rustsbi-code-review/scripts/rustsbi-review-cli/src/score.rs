//! Historical case scoring, separate from the review pipeline.

use anyhow::{ensure, Context, Result};
use regex::Regex;
use serde_json::Value;
use std::{collections::BTreeSet, fs};

struct Anchor {
    file: String,
    line: Option<u64>,
    text: String,
}

fn same_file(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().trim_start_matches("./").replace('\\', "/");
    let (a, b) = (norm(a), norm(b));
    a == b || a.ends_with(&format!("/{b}")) || b.ends_with(&format!("/{a}"))
}

fn run(args: &[String]) -> Result<()> {
    let (tol, args) = if args.first().is_some_and(|a| a == "--tol") {
        (
            args.get(1).context("missing tolerance")?.parse::<u64>()?,
            &args[2..],
        )
    } else {
        (5, args)
    };
    ensure!(args.len() == 2, "usage: score [--tol N] CASE COMMENTS.json");
    let heading = Regex::new(r"^### `(.+?)`(?: line ([0-9]+))?\s*$")?;
    let class = Regex::new(r"^\[([a-z-]+)\]")?;
    let mut ground: Vec<Anchor> = Vec::new();
    for line in fs::read_to_string(&args[0])?.lines() {
        if let Some(m) = heading.captures(line) {
            ground.push(Anchor {
                file: m[1].into(),
                line: m.get(2).map(|m| m.as_str().parse()).transpose()?,
                text: String::new(),
            });
        } else if let (Some(text), Some(anchor)) = (line.strip_prefix('>'), ground.last_mut()) {
            if !anchor.text.is_empty() {
                anchor.text.push(' ');
            }
            anchor.text.push_str(text.trim());
        }
    }
    let scored: Vec<_> = ground
        .iter()
        .filter(|g| {
            class
                .captures(&g.text)
                .is_none_or(|c| &c[1] == "code-derivable")
        })
        .collect();
    let produced: Value = serde_json::from_slice(&fs::read(&args[1])?)?;
    let produced = produced
        .as_array()
        .or_else(|| produced.get("comments").and_then(Value::as_array))
        .context("expected comments array")?;
    let mut hit_ground = BTreeSet::new();
    let mut hit_produced = BTreeSet::new();
    for (i, g) in scored.iter().enumerate() {
        for (j, p) in produced.iter().enumerate() {
            if hit_produced.contains(&j) || !same_file(&g.file, p["file"].as_str().unwrap_or("")) {
                continue;
            }
            if let (Some(a), Some(b)) = (g.line, p["line"].as_u64()) {
                if a.abs_diff(b) > tol {
                    continue;
                }
            }
            hit_ground.insert(i);
            hit_produced.insert(j);
        }
    }
    let ratio = |hits: usize, total: usize| {
        if total == 0 {
            0.0
        } else {
            hits as f64 * 100.0 / total as f64
        }
    };
    println!(
        "ground truth: {} total, {} code-derivable",
        ground.len(),
        scored.len()
    );
    println!(
        "produced: {}\nrecall: {:.1}%\nprecision: {:.1}%",
        produced.len(),
        ratio(hit_ground.len(), scored.len()),
        ratio(hit_produced.len(), produced.len())
    );
    for (i, g) in scored
        .iter()
        .enumerate()
        .filter(|(i, _)| !hit_ground.contains(i))
    {
        let _ = i;
        println!("not reproduced: {} line {:?}", g.file, g.line);
    }
    Ok(())
}

pub fn score_main(args: &[String]) -> i32 {
    match run(args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("{e:#}");
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn matches_only_same_file_suffixes() {
        assert!(same_file("./src/lib.rs", "lib.rs"));
        assert!(!same_file("src/notlib.rs", "lib.rs"));
    }
}
