//! Shared data types: check results, stages, statuses, and the resolved scope.

use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Diff,
    Files,
    Pr,
}

impl Mode {
    pub fn as_str(self) -> &'static str {
        match self {
            Mode::Diff => "diff",
            Mode::Files => "files",
            Mode::Pr => "pr",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pass,
    Fail,
    Skip,
    Error,
    Timeout,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::Skip => "skip",
            Status::Error => "error",
            Status::Timeout => "timeout",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Status::Pass => "OK",
            Status::Fail => "FAIL",
            Status::Skip => "SKIP",
            Status::Error => "ERR",
            Status::Timeout => "TIMEOUT",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Gate {
    /// Blocks the LLM review on failure.
    Compile,
    /// Report only.
    Advisory,
}

#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Scope,
    Convention,
    Build,
    Lint,
    Test,
}

/// One check: a command or analysis, with its outcome and timing.
#[derive(Clone, Serialize)]
pub struct CheckResult {
    pub id: String,
    pub stage: Stage,
    pub status: Status,
    pub gate: Gate,
    pub duration_ms: u64,
    pub command: String,
    pub message: String,
    /// Full output (kept truncated to ~4KB).
    pub output: String,
}

impl CheckResult {
    pub fn new(
        id: &str,
        stage: Stage,
        status: Status,
        gate: Gate,
        duration_ms: u64,
        command: &str,
        message: &str,
    ) -> Self {
        CheckResult {
            id: id.to_string(),
            stage,
            status,
            gate,
            duration_ms,
            command: command.to_string(),
            message: message.to_string(),
            output: String::new(),
        }
    }

    pub fn with_output(mut self, output: &str) -> Self {
        self.output = crate::util::truncate(output, 4096);
        self
    }

    /// A compile-gated check that failed or timed out blocks the review.
    pub fn is_compile_fail(&self) -> bool {
        self.gate == Gate::Compile && matches!(self.status, Status::Fail | Status::Timeout)
    }
}

/// Resolved target and scope for the check pipeline.
#[derive(Clone)]
pub struct ScopeInfo {
    pub mode: Mode,
    pub base: String,
    pub head: String,
    pub branch: String,
    pub changed_paths: Vec<String>,
    /// cargo package names affected by the change.
    pub affected_packages: Vec<String>,
    pub dirty: bool,
    /// Workspace-level change: check everything.
    pub all_packages: bool,
    /// Short sha256 used for caching.
    pub fingerprint: String,
    /// Non-empty = skip all checks (e.g. pr/N backtest).
    pub skipped_reason: String,
}

impl ScopeInfo {
    pub fn new(mode: Mode) -> Self {
        ScopeInfo {
            mode,
            base: String::new(),
            head: String::new(),
            branch: String::new(),
            changed_paths: Vec::new(),
            affected_packages: Vec::new(),
            dirty: false,
            all_packages: false,
            fingerprint: String::new(),
            skipped_reason: String::new(),
        }
    }

    pub fn mode_str(&self) -> &'static str {
        self.mode.as_str()
    }
}
