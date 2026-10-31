"""convention.py - format, whitespace, DCO, and changelog checks.

These are non-compiling checks that verify the change follows
RustSBI's coding and commit conventions.
"""

import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from __init__ import CheckResult, run_cmd, first_line, truncate
from scope import git, git_env

NL = chr(10)

# Library directories that require a CHANGELOG.md update (from Changelog.yml)
CHANGELOG_DIRS = [
    "library/sbi-rt",
    "library/sbi-spec",
    "library/sbi-testing",
    "library/rustsbi",
    "library/macros",
]


def check_fmt(scope):
    """cargo fmt --all -- --check"""
    rc, out, err, ms = run_cmd(
        ["cargo", "fmt", "--all", "--", "--check"], timeout=120)
    # Parse "Diff in <file> at line <n>" for a summary
    diffs = [l for l in out.splitlines() if l.startswith("Diff in ")]
    if rc == 0:
        return CheckResult("fmt", "convention", "pass", "advisory", ms,
                           "cargo fmt --all -- --check", "ok")
    msg = "{} file(s) need formatting".format(len(diffs)) if diffs else first_line(out or err)
    return CheckResult("fmt", "convention", "fail", "advisory", ms,
                       "cargo fmt --all -- --check", msg, truncate(out or err))


def check_whitespace(scope):
    """git diff --check with autocrlf to handle Windows CRLF."""
    if scope.mode != "diff" or not scope.base:
        return CheckResult("whitespace", "convention", "skip", "advisory", 0,
                           "", "not applicable (files mode)")
    rc, mb, _, _ = git(["merge-base", scope.base, "HEAD"])
    mb = mb.strip()
    rc, out, err, ms = git(["diff", "--check", mb + "..HEAD"])
    if rc == 0:
        return CheckResult("whitespace", "convention", "pass", "advisory", ms,
                           "git diff --check", "ok")
    lines = [l for l in out.splitlines() if l.strip() and not l.startswith("warning:")]
    msg = "{} whitespace error(s)".format(len(lines)) if lines else first_line(err)
    return CheckResult("whitespace", "convention", "fail", "advisory", ms,
                       "git diff --check", msg, truncate(out))


def check_dco(scope):
    """Verify Signed-off-by trailers match author or committer.

    Mirrors .github/workflows/DCO.yml logic.
    """
    if scope.mode != "diff" or not scope.base:
        return CheckResult("dco", "convention", "skip", "advisory", 0,
                           "", "not applicable (files mode)")
    rc, mb, _, _ = git(["merge-base", scope.base, "HEAD"])
    mb = mb.strip()
    rc, out, err, ms = git(["rev-list", mb + "..HEAD"])
    if rc != 0:
        return CheckResult("dco", "convention", "error", "advisory", ms,
                           "git rev-list", first_line(err))
    commits = [c.strip() for c in out.splitlines() if c.strip()]

    failed = []
    for c in commits:
        # Skip merge commits
        rc, parents, _, _ = git(["show", "-s", "--format=%P", c])
        if len(parents.split()) > 1:
            continue
        # Get author and committer
        rc, ids, _, _ = git(["show", "-s", "--format=%an%x00%ae%x00%cn%x00%ce", c])
        parts = ids.strip().split("\0")
        if len(parts) < 4:
            failed.append(c[:7] + " (no identity)")
            continue
        author = (parts[0].lower(), parts[1].lower())
        committer = (parts[2].lower(), parts[3].lower())

        # Parse message for Signed-off-by trailers
        rc, msg, _, _ = git(["show", "-s", "--format=%B", c])
        signoffs = []
        for line in msg.splitlines():
            line = line.strip()
            if line.lower().startswith("signed-off-by:"):
                val = line[len("signed-off-by:"):].strip()
                m = re.match(r"^(.+?)\s*<(.+?)>$", val)
                if m:
                    signoffs.append((m.group(1).strip().lower(), m.group(2).strip().lower()))

        if not any(s in (author, committer) for s in signoffs):
            failed.append(c[:7])

    total_ms = ms  # rough; individual git calls not timed separately
    if failed:
        return CheckResult("dco", "convention", "fail", "advisory", total_ms,
                           "python: DCO check",
                           "{} of {} commit(s) missing valid Signed-off-by".format(
                               len(failed), len(commits)),
                           "Missing sign-off: " + ", ".join(failed))
    return CheckResult("dco", "convention", "pass", "advisory", total_ms,
                       "python: DCO check",
                       "{} commit(s) verified".format(len(commits)))


def check_changelog(scope):
    """Check that library changes include a CHANGELOG.md update.

    Mirrors .github/workflows/Changelog.yml logic.
    """
    if scope.mode != "diff" or not scope.base:
        return CheckResult("changelog", "convention", "skip", "advisory", 0,
                           "", "not applicable (files mode)")
    rc, mb, _, _ = git(["merge-base", scope.base, "HEAD"])
    mb = mb.strip()
    rc, out, err, ms = git(["diff", "--name-only", mb + "..HEAD"])
    changed = set(l.strip() for l in out.splitlines() if l.strip())

    missing = []
    for d in CHANGELOG_DIRS:
        if any(p.startswith(d + "/") for p in changed):
            cl = d + "/CHANGELOG.md"
            if cl not in changed:
                missing.append(cl)

    if missing:
        return CheckResult("changelog", "convention", "fail", "advisory", ms,
                           "python: changelog check",
                           "{} changelog(s) not updated".format(len(missing)),
                           "Missing: " + ", ".join(missing))
    return CheckResult("changelog", "convention", "pass", "advisory", ms,
                       "python: changelog check", "ok")


def run_convention(scope):
    """Run all convention checks. Returns list of CheckResult."""
    results = []
    results.append(check_fmt(scope))
    results.append(check_whitespace(scope))
    results.append(check_dco(scope))
    results.append(check_changelog(scope))
    return results
