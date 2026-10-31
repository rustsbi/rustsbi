#!/usr/bin/env python3
"""cli.py - entry point for the deterministic check pipeline.

Stages: scope -> convention -> build -> lint -> test -> report

Usage:
    cli.py '<raw args>' <output>

Raw args are the same string resolve_target.py receives:
    diff   <base|pr/N>   <output>
    files  <path ...>    <output>

Outputs:
    <output>.checks.json   machine-readable results
    <output>.checks.md     human-readable report
"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from __init__ import CheckResult, die, NL
from scope import run_scope
from convention import run_convention
from build import run_build
from lint import run_lint
from test_stage import run_test
from report import write_report


def main():
    argv = sys.argv[1:]
    if len(argv) < 2:
        die("usage: cli.py '<raw args>' <output>")
    raw_args, output = argv[0], argv[1]

    # Stage 1: scope
    scope = run_scope(raw_args)
    if scope.skipped_reason:
        # Backtest mode (pr/N): skip all checks
        results = [CheckResult("all", "scope", "skip", "compile", 0,
                                "", scope.skipped_reason)]
        json_path, md_path, allowed, overall = write_report(output, scope, results)
        print("{}: skipped ({})".format(json_path, scope.skipped_reason))
        return 0

    print("scope: mode={} head={} packages={} dirty={}".format(
        scope.mode, scope.head, scope.affected_packages, scope.dirty))

    all_results = []

    # Stage 2: convention
    print("convention: running...")
    convention_results = run_convention(scope)
    all_results.extend(convention_results)
    for r in convention_results:
        print("  {}: {} ({})".format(r.id, r.status, r.duration_ms))

    # Stage 3: build
    print("build: running...")
    build_results = run_build(scope)
    all_results.extend(build_results)
    for r in build_results:
        print("  {}: {} ({})".format(r.id, r.status, r.duration_ms))

    # Check if compile failed - if so, skip lint and test
    compile_failed = any(
        r.gate == "compile" and r.status in ("fail", "timeout")
        for r in build_results)

    if compile_failed:
        print("build: compile failure, skipping lint and test")
        skip_r = CheckResult("lint-test", "lint", "skip", "advisory", 0,
                             "", "skipped: compile failure")
        all_results.append(skip_r)
    else:
        # Stage 4: lint
        print("lint: running...")
        lint_results = run_lint(scope)
        all_results.extend(lint_results)
        for r in lint_results:
            print("  {}: {} ({})".format(r.id, r.status, r.duration_ms))

        # Stage 5: test
        print("test: running...")
        test_results = run_test(scope)
        all_results.extend(test_results)
        for r in test_results:
            print("  {}: {} ({})".format(r.id, r.status, r.duration_ms))

    # Stage 6: report
    json_path, md_path, review_allowed, overall = write_report(
        output, scope, all_results)

    total_ms = sum(r.duration_ms for r in all_results)
    print("{}".format(NL.join([
        "checks: {} overall={} review_allowed={}".format(
            json_path, overall, review_allowed),
        "report: {}".format(md_path),
        "total: {:.1f}s, {} check(s)".format(total_ms / 1000, len(all_results)),
    ])))

    return 0 if review_allowed else 1


if __name__ == "__main__":
    sys.exit(main())