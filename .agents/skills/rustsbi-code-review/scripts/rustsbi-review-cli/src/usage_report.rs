//! usage subcommand — token accounting across review runs.
//! Rust port of scripts/usage.py.
//!
//! Only tokens, never prices: the agent CLI's own envelope supplies them.
//! The sidecar files keep raw integers; this report prints k/M scaled values.

use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Default)]
struct Counters {
    cache_write: i64,
    cache_read: i64,
    plain_read: i64,
    write: i64,
    total_read: i64,
}

fn human(n: i64) -> String {
    let a = n.abs();
    if a >= 1_000_000 {
        format!("{:.2}M", n as f64 / 1_000_000.0)
    } else if a >= 1_000 {
        format!("{:.2}k", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

fn counters(usage: Option<&Value>) -> Counters {
    let u = usage.unwrap_or(&Value::Null);
    let get = |k: &str| u.get(k).and_then(Value::as_i64).unwrap_or(0);
    let mut c = Counters {
        cache_write: get("cache_creation_input_tokens"),
        cache_read: get("cache_read_input_tokens"),
        plain_read: get("input_tokens"),
        write: get("output_tokens"),
        total_read: 0,
    };
    c.total_read = c.cache_write + c.cache_read + c.plain_read;
    c
}

fn load(pattern: &str) -> Vec<(String, Value)> {
    let mut paths: Vec<String> = Vec::new();
    if let Ok(g) = glob::glob(pattern) {
        paths.extend(g.flatten().map(|p| p.to_string_lossy().into_owned()));
    }
    if paths.is_empty() {
        if let Ok(g) = glob::glob(&format!("{}*", pattern)) {
            paths.extend(g.flatten().map(|p| p.to_string_lossy().into_owned()));
        }
    }
    paths.sort();
    paths
        .into_iter()
        .filter(|p| p.ends_with(".usage.json"))
        .filter_map(|p| {
            std::fs::read_to_string(&p).ok().and_then(|s| serde_json::from_str(&s).ok()).map(|v| (p, v))
        })
        .collect()
}

pub fn usage_main(argv: &[String]) -> i32 {
    if argv.is_empty() {
        eprintln!("usage: usage <path-or-glob> [...]");
        return 1;
    }
    struct Row {
        path: String,
        persona: String,
        mode: String,
        c: Counters,
    }
    let mut rows: Vec<Row> = Vec::new();
    for pattern in argv {
        for (path, u) in load(pattern) {
            if let Some(list) = u.get("passes").and_then(Value::as_array) {
                for p in list {
                    rows.push(Row {
                        path: path.clone(),
                        persona: p["persona"].as_str().unwrap_or("-").to_string(),
                        mode: u["mode"].as_str().unwrap_or("-").to_string(),
                        c: counters(p.get("usage")),
                    });
                }
            } else {
                let personas = u.get("personas").and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>().join(","))
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "-".to_string());
                rows.push(Row {
                    path,
                    persona: personas,
                    mode: u["mode"].as_str().unwrap_or("combined").to_string(),
                    c: counters(u.get("usage")),
                });
            }
        }
    }
    if rows.is_empty() {
        eprintln!("no .usage.json found");
        return 1;
    }

    println!("{:<34} {:<8} {:<18} {:>10} {:>10} {:>10} {:>11} {:>9}",
        "run", "mode", "persona", "cache_wr", "cache_rd", "plain_rd", "total_read", "write");
    for r in &rows {
        let tail: String = r.path.chars().rev().take(34).collect::<Vec<_>>().into_iter().rev().collect();
        println!("{:<34} {:<8} {:<18} {:>10} {:>10} {:>10} {:>11} {:>9}",
            tail, r.mode, r.persona,
            human(r.c.cache_write), human(r.c.cache_read), human(r.c.plain_read),
            human(r.c.total_read), human(r.c.write));
    }
    let mut agg: BTreeMap<&str, i64> = BTreeMap::new();
    for r in &rows {
        *agg.entry("cache_write").or_default() += r.c.cache_write;
        *agg.entry("cache_read").or_default() += r.c.cache_read;
        *agg.entry("plain_read").or_default() += r.c.plain_read;
        *agg.entry("total_read").or_default() += r.c.total_read;
        *agg.entry("write").or_default() += r.c.write;
    }
    println!("{:<34} {:<8} {:<18} {:>10} {:>10} {:>10} {:>11} {:>9}",
        format!("TOTAL ({} passes)", rows.len()), "", "",
        human(agg["cache_write"]), human(agg["cache_read"]), human(agg["plain_read"]),
        human(agg["total_read"]), human(agg["write"]));
    0
}