// SPDX-License-Identifier: MulanPSL-2.0

use crate::{Result, git};
use regex::Regex;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize)]
pub struct Identity {
    pub name: String,
    pub email: String,
}

impl Identity {
    pub fn parse(value: &str) -> Result<Self> {
        let (name, tail) = value.rsplit_once(" <").ok_or("invalid Git identity")?;
        let (email, _) = tail.split_once('>').ok_or("invalid Git identity email")?;
        if name.trim().is_empty()
            || email.is_empty()
            || !email.contains('@')
            || email
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '<' | '>'))
        {
            return Err("missing or invalid name/email; configure your own identity".into());
        }
        Ok(Self {
            name: name.into(),
            email: email.into(),
        })
    }

    fn matches(&self, value: &str) -> bool {
        value.eq_ignore_ascii_case(&format!("{} <{}>", self.name, self.email))
    }
}

pub fn noreply(value: &str) -> bool {
    value
        .to_ascii_lowercase()
        .replace(['-', '_', '.'], "")
        .contains("noreply")
}

pub fn check(
    repo: &Path,
    message: &str,
    author: &Identity,
    committer: &Identity,
) -> Result<Vec<String>> {
    let mut errors = Vec::new();
    for (kind, identity) in [("author", author), ("committer", committer)] {
        if noreply(&identity.email) {
            errors.push(format!("{kind} uses a noreply email: {}", identity.email));
        }
    }
    if message.starts_with('\u{feff}') || message.contains('\r') || message.contains('\0') {
        errors.push("message must be UTF-8 without BOM, CR or NUL".into());
    }
    let lines: Vec<_> = message.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if line.chars().count() > 100 {
            errors.push(format!("message line {} exceeds 100 characters", index + 1));
        }
        if line.trim_end() != *line {
            errors.push(format!(
                "message line {} has trailing whitespace",
                index + 1
            ));
        }
    }
    let header = lines.first().copied().unwrap_or("");
    if header.chars().count() > 72 {
        errors
            .push("complete subject/header line exceeds 72 characters (prefer at most 50)".into());
    }
    let pattern = Regex::new(r"^([a-z][a-z0-9-]*)(?:\(([a-z0-9][a-z0-9_./-]*)\))?!?: (\S.*)$")?;
    if let Some(captures) = pattern.captures(header) {
        let subject = &captures[3];
        if subject.ends_with('.') {
            errors.push("subject must not end with a period".into());
        }
        if captures.get(2).is_some()
            && !["feat", "fix", "build", "refactor", "docs", "ci", "test"].contains(&&captures[1])
        {
            errors.push(
                "type(scope) must use a documented type; use scope: for a module prefix".into(),
            );
        }
    } else {
        errors.push("header must be type(scope): subject or scope/type: subject".into());
    }
    if lines.get(1) != Some(&"") {
        errors.push("keep the second message line blank".into());
    }
    let trailers = String::from_utf8(git::bytes(
        repo,
        &["interpret-trailers", "--parse"],
        Some(message.as_bytes()),
    )?)?;
    let mut signed = false;
    for line in trailers.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let value = value.trim();
            if key.eq_ignore_ascii_case("signed-off-by") {
                signed |= author.matches(value) || committer.matches(value);
                if noreply(value) {
                    errors.push("a Signed-off-by trailer uses a noreply email".into());
                }
            }
        }
    }
    if !signed {
        errors.push(
            "missing final Signed-off-by trailer matching the effective author or committer".into(),
        );
    }
    // Catch attribution in prose as well as trailers, without rejecting real
    // human Co-authored-by trailers or ordinary mentions of a tool in the body.
    let attribution = Regex::new(
        r"(?i)(co[ -]?authored[ -]?by|generated[ -]?by|assisted[ -]?by|written[ -]?by|signed[ -]?off[ -]?by)\s*:",
    )?;
    let tool = Regex::new(
        r"(?i)\b(claude|codex|anthropic|openai|chatgpt|copilot|gemini|cursor|aider|deepseek|qwen|llm|ai|gpt[-\w.]*)\b|\[bot\]",
    )?;
    let credit =
        Regex::new(r"(?i)\b(generated|written|co[ -]?authored|assisted)\s+(with|by|using)\b")?;
    for (kind, identity) in [("author", author), ("committer", committer)] {
        if tool.is_match(&format!("{} <{}>", identity.name, identity.email)) {
            errors.push(format!(
                "{kind} must be a human developer, not a tool/bot identity"
            ));
        }
    }
    for line in &lines {
        if credit.is_match(line) && tool.is_match(line) {
            errors.push("remove tool/bot authorship or generation attribution".into());
            continue;
        }
        if let Some(found) = attribution.find(line) {
            let key = found.as_str().to_ascii_lowercase();
            if key.starts_with("generated")
                || key.starts_with("assisted")
                || (tool.is_match(&line[found.end()..]))
            {
                errors.push("remove tool/bot authorship or generation attribution".into());
            }
        }
    }
    let body_end = message
        .trim_end()
        .rfind("\n\n")
        .map(|p| &message[..p])
        .unwrap_or(header);
    if body_end.lines().skip(2).all(|line| line.trim().is_empty()) {
        errors.push(
            "direct contributions require a body explaining the change and its reason".into(),
        );
    }
    if message.lines().any(|line| {
        line.chars().any(|c| {
            matches!(c as u32,
        0x3400..=0x9fff | 0x3040..=0x30ff | 0xac00..=0xd7af)
        }) && !line.to_ascii_lowercase().starts_with("signed-off-by:")
            && !line.to_ascii_lowercase().starts_with("co-authored-by:")
    }) {
        errors.push("write the subject and explanatory prose in English".into());
    }
    Ok(errors)
}
