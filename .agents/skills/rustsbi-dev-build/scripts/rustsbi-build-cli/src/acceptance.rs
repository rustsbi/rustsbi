// SPDX-License-Identifier: MulanPSL-2.0

use serde_json::{Value, json};
use std::{fs, path::Path};

pub fn validate(report: &Value, repo: &Path, pi_status: i32) -> Vec<String> {
    let mut errors = Vec::new();
    if pi_status != 0 {
        errors.push(format!("pi exited with status {pi_status}"));
    }
    let plan = &report["plan"];
    let expected_repo = repo.canonicalize().ok();
    if report["status"] != "passed" || plan["mode"] != "run" {
        errors.push("build did not pass".into());
    }
    if expected_repo.is_none()
        || plan["suite"] != "all"
        || plan["repo"]
            .as_str()
            .and_then(|p| Path::new(p).canonicalize().ok())
            != expected_repo
    {
        errors.push("all suite did not run in the selected checkout".into());
    }
    let planned = plan["steps"].as_array();
    let actual = report["steps"].as_array();
    match (planned, actual) {
        (Some(planned), Some(actual)) if !actual.is_empty() => {
            if planned
                .iter()
                .chain(actual)
                .any(|s| s["id"].as_str().is_none())
                || planned.iter().map(|s| &s["id"]).collect::<Vec<_>>()
                    != actual.iter().map(|s| &s["id"]).collect::<Vec<_>>()
            {
                errors.push("incomplete step results".into());
            }
            if actual
                .iter()
                .any(|s| s["status"] != "passed" || s["exit_code"] != 0)
            {
                errors.push("not every step passed".into());
            }
            if report["boot_verified"] != true
                || planned
                    .iter()
                    .filter(|s| s["verifies_boot"] == true)
                    .count()
                    != 6
            {
                errors.push("six boot checks not verified".into());
            }
        }
        _ => errors.push("missing step arrays".into()),
    }
    errors
}

pub fn verify(
    output: &Path,
    repo: &Path,
    pi_status: i32,
) -> Result<u8, Box<dyn std::error::Error>> {
    let errors = match fs::read(output.join("build/report.json"))
        .map_err(Into::into)
        .and_then(|b| {
            serde_json::from_slice::<Value>(&b).map_err(Box::<dyn std::error::Error>::from)
        }) {
        Ok(report) => validate(&report, repo, pi_status),
        Err(error) => vec![format!("missing or invalid build report: {error}")],
    };
    let result = json!({"status": if errors.is_empty() {"passed"} else {"failed"}, "pi_exit_code": pi_status, "errors": errors});
    fs::write(
        output.join("acceptance.json"),
        serde_json::to_string_pretty(&result)? + "\n",
    )?;
    println!("{result}");
    Ok(u8::from(!errors.is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_success_does_not_hide_incomplete_or_failed_boots() {
        let repo = std::env::current_dir().unwrap();
        let steps: Vec<_> = (0..6)
            .map(|i| json!({"id": i.to_string(), "verifies_boot": true}))
            .collect();
        let results: Vec<_> = (0..6)
            .map(|i| json!({"id": i.to_string(), "status": "passed", "exit_code": 0}))
            .collect();
        let mut report = json!({"status":"passed", "boot_verified":true, "plan":{"mode":"run", "suite":"all", "repo":repo, "steps":steps}, "steps":results});
        assert!(validate(&report, &repo, 0).is_empty());
        assert!(!validate(&report, &repo, 1).is_empty());
        let missing = repo.join("missing-acceptance-checkout");
        let mut malformed = report.clone();
        malformed["plan"]["repo"] = json!(missing);
        assert!(!validate(&malformed, &missing, 0).is_empty());
        malformed = report.clone();
        malformed["steps"][0]["id"] = Value::Null;
        assert!(!validate(&malformed, &repo, 0).is_empty());
        report["steps"].as_array_mut().unwrap().pop();
        assert!(!validate(&report, &repo, 0).is_empty());
        report["status"] = json!("blocked");
        report["boot_verified"] = json!(false);
        assert!(!validate(&report, &repo, 0).is_empty());
    }
}
