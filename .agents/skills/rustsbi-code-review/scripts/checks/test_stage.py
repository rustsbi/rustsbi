"""test_stage.py - cargo test for affected packages.

Test failures are reported but do not block the LLM review:
a failing test is a finding, not a reason to skip semantic review.
"""

import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from __init__ import CheckResult, run_cmd, first_line, truncate

NL = chr(10)

# rustsbi tests run both with and without the machine feature (from Library.yml)
RUSTSBI_TEST_FEATURES = [[], ["machine"]]


def check_test(pkg, features=None, lib_only=False):
    """cargo test for one package."""
    args = ["cargo", "test", "--locked", "-p", pkg]
    if lib_only:
        args.append("--lib")
    if features:
        args += ["--features", features]
    cmd_str = " ".join(args)
    rc, out, err, ms = run_cmd(args, timeout=600)
    if rc == 0:
        passed = sum(1 for l in out.splitlines() if "test result: ok" in l)
        return CheckResult("test-" + pkg + ("-features" if features else ""),
                           "test", "pass", "advisory", ms, cmd_str,
                           "{} test suite(s) passed".format(passed))
    return CheckResult("test-" + pkg + ("-features" if features else ""),
                       "test", "fail", "advisory", ms, cmd_str,
                       first_line(err or out), truncate(err or out))


def run_test(scope):
    """Run test checks for affected packages."""
    if scope.skipped_reason:
        return [CheckResult("test", "test", "skip", "advisory", 0,
                            "", scope.skipped_reason)]
    if not scope.affected_packages:
        return [CheckResult("test", "test", "skip", "advisory", 0,
                            "", "no affected packages")]

    results = []
    for pkg in scope.affected_packages:
        if pkg == "rustsbi":
            for feats in RUSTSBI_TEST_FEATURES:
                feat_str = ",".join(feats) if feats else ""
                results.append(check_test(pkg, feat_str))
        elif pkg == "rustsbi-firmware-runtime":
            # CI convention: --lib only (firmware doc tests need RISC-V)
            results.append(check_test(pkg, lib_only=True))
        elif pkg == "rustsbi-prototyper":
            # Prototyper bin has test=false; only macros are tested
            results.append(check_test("rustsbi-runtime-macros"))
        elif pkg == "xtask":
            results.append(check_test(pkg))
        else:
            results.append(check_test(pkg))
    return results