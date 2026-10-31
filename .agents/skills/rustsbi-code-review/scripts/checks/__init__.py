"""Shared types and helpers for the deterministic check stages.

Stages: scope -> convention -> build -> lint -> test -> report
"""

import os
import subprocess
import time
from dataclasses import dataclass, field
from typing import Optional

NL = chr(10)


def die(msg):
    import sys
    sys.stderr.write(msg.rstrip() + NL)
    sys.exit(2)


def run_cmd(args, cwd=None, timeout=600, env_extra=None):
    """Run a command, return (returncode, stdout, stderr, duration_ms).

    Never raises on non-zero exit; the caller decides what to do.
    Raises TimeoutExpired if the command exceeds the timeout.
    """
    env = os.environ.copy()
    if env_extra:
        env.update(env_extra)
    start = time.monotonic()
    p = subprocess.run(
        args, capture_output=True, text=True,
        encoding="utf-8", errors="replace",
        cwd=cwd, timeout=timeout, env=env)
    ms = int((time.monotonic() - start) * 1000)
    return p.returncode, p.stdout or "", p.stderr or "", ms


@dataclass
class CheckResult:
    """One check: a command or analysis, with its outcome and timing."""
    id: str                     # e.g. "fmt", "dco", "check-rustsbi-rv64"
    stage: str                  # "convention", "build", "lint", "test"
    status: str                 # "pass", "fail", "skip", "error", "timeout"
    gate: str                   # "compile" (blocks review) or "advisory" (report only)
    duration_ms: int
    command: str                # what was run (or "python:" for internal checks)
    message: str                # first meaningful line of output
    output: str = ""            # full output (truncated to ~4KB by report)

    def to_dict(self):
        return {
            "id": self.id, "stage": self.stage, "status": self.status,
            "gate": self.gate, "duration_ms": self.duration_ms,
            "command": self.command, "message": self.message,
            "output": self.output[:4096],
        }


@dataclass
class ScopeInfo:
    """Resolved target and scope for the check pipeline."""
    mode: str                           # "diff", "files", "pr"
    base: str = ""                      # base ref (diff mode)
    head: str = ""                      # head sha
    branch: str = ""
    changed_paths: list = field(default_factory=list)
    affected_packages: list = field(default_factory=list)  # cargo package names
    dirty: bool = False                 # working tree dirty
    all_packages: bool = False          # workspace-level change, check everything
    fingerprint: str = ""               # sha256 for cache
    skipped_reason: str = ""            # non-empty = skip all checks (e.g. pr/N backtest)


def first_line(text, max_len=200):
    """Return the first non-empty line of output, truncated."""
    for line in text.splitlines():
        line = line.strip()
        if line:
            return line[:max_len]
    return ""


def truncate(text, limit=4096):
    if len(text) <= limit:
        return text
    return text[:limit] + NL + "... (truncated)"
