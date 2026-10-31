"""lint.py - clippy and typos checks (advisory by default).

Clippy findings are advisory: they don't block the review,
but they appear in the report so the LLM pass doesn't duplicate them.
"""

import os
import shutil
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from __init__ import CheckResult, run_cmd, first_line, truncate

NL = chr(10)
RV64 = "riscv64imac-unknown-none-elf"
RV64GC = "riscv64gc-unknown-none-elf"


def check_clippy(pkg, target=None):
    """cargo clippy for one package. Advisory: warnings don't fail."""
    args = ["cargo", "clippy", "--locked", "-p", pkg]
    if target:
        args += ["--target", target]
    cmd_str = " ".join(args)
    rc, out, err, ms = run_cmd(args, timeout=600)
    warnings = [l for l in (out + err).splitlines()
                if l.startswith("warning:") and "generated" not in l]
    if rc == 0 and not warnings:
        return CheckResult("clippy-" + pkg, "lint", "pass", "advisory", ms,
                           cmd_str, "ok")
    if rc != 0 and "error" in (out + err).lower():
        return CheckResult("clippy-" + pkg, "lint", "error", "advisory", ms,
                           cmd_str, first_line(err or out), truncate(err or out))
    if warnings:
        return CheckResult("clippy-" + pkg, "lint", "fail", "advisory", ms,
                           cmd_str, "{} warning(s)".format(len(warnings)),
                           truncate(NL.join(warnings[:20])))
    return CheckResult("clippy-" + pkg, "lint", "pass", "advisory", ms,
                       cmd_str, "ok")


def check_typos(scope):
    """typos CLI if available. Advisory."""
    if not shutil.which("typos"):
        return CheckResult("typos", "lint", "skip", "advisory", 0,
                           "", "typos not installed")
    rc, out, err, ms = run_cmd(["typos"], timeout=120)
    if rc == 0:
        return CheckResult("typos", "lint", "pass", "advisory", ms,
                           "typos", "ok")
    return CheckResult("typos", "lint", "fail", "advisory", ms,
                       "typos", first_line(out or err), truncate(out or err))


# Packages that only build on the host (not for RISC-V targets)
HOST_ONLY = {"xtask"}


def run_lint(scope):
    """Run lint checks for affected packages."""
    if scope.skipped_reason:
        return [CheckResult("lint", "lint", "skip", "advisory", 0,
                            "", scope.skipped_reason)]
    if not scope.affected_packages:
        return [CheckResult("lint", "lint", "skip", "advisory", 0,
                            "", "no affected packages")]

    results = []
    for pkg in scope.affected_packages:
        if pkg in HOST_ONLY:
            results.append(check_clippy(pkg, target=None))
        elif pkg == "rustsbi-prototyper":
            results.append(check_clippy(pkg, target=RV64GC))
        else:
            results.append(check_clippy(pkg, target=RV64))
    results.append(check_typos(scope))
    return results
