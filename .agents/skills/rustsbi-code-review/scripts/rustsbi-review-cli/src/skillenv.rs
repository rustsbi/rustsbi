//! Locate the skill home and the persona table.

use std::path::PathBuf;

/// (persona name, guideline index path relative to the guidelines root).
pub const PERSONAS: [(&str, &str); 3] = [
    ("maintainability", "for-maintainability/README.md"),
    ("development", "for-development/README.md"),
    ("documentation", "for-documentation/README.md"),
];

/// The skill directory (the one holding pass_contract.md).
///
/// Resolution order: $RCR_SKILL_HOME, then walking up from the executable
/// (works for target/release binaries and `cargo run` alike), then the
/// cwd-relative layout of the skill repo.
pub fn skill_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("RCR_SKILL_HOME") {
        let p = PathBuf::from(p);
        if p.join("pass_contract.md").is_file() {
            return p;
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        for dir in exe.ancestors().skip(1) {
            if dir.join("pass_contract.md").is_file() {
                return dir.to_path_buf();
            }
        }
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let p = cwd.join(".agents/skills/rustsbi-code-review");
    if p.join("pass_contract.md").is_file() {
        return p;
    }
    // Last resort: the conventional skill checkout location.
    PathBuf::from(".agents/skills/rustsbi-code-review")
}

/// Guidelines root: $RCR_GUIDELINES_ROOT or the in-repo default.
pub fn guidelines_root() -> String {
    std::env::var("RCR_GUIDELINES_ROOT").unwrap_or_else(|_| "docs/src/guidelines".to_string())
}