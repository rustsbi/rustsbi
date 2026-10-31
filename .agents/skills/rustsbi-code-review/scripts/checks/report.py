"""report.py - aggregate results into checks.json and checks.md.

Determines review_allowed: true unless a compile-gated check failed.
Writes machine-readable JSON and human-readable Markdown.
"""

import json
import os
import sys
from datetime import datetime, timezone

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

NL = chr(10)
BT = chr(96)
FENCE = BT * 3

STAGE_ORDER = ["scope", "convention", "build", "lint", "test", "report"]

STATUS_ICONS = {
    "pass": "OK",
    "fail": "FAIL",
    "skip": "SKIP",
    "error": "ERR",
    "timeout": "TIMEOUT",
}


def determine_verdict(results):
    """Return (review_allowed, overall_status, blocking_reason)."""
    compile_fails = [r for r in results
                     if r.gate == "compile" and r.status in ("fail", "timeout")]
    if compile_fails:
        reasons = "; ".join(r.id + ": " + r.message for r in compile_fails[:3])
        return False, "blocked", "compile failure: " + reasons
    any_fail = [r for r in results if r.status == "fail"]
    if any_fail:
        return True, "failed", ""
    return True, "passed", ""


def render_md(scope, results, review_allowed, overall, blocking_reason):
    lines = ["# Deterministic Checks Report", ""]
    lines.append("## Summary")
    lines.append("")
    lines.append("- **overall**: " + BT + overall + BT)
    lines.append("- **review_allowed**: " + BT + str(review_allowed).lower() + BT)
    if blocking_reason:
        lines.append("- **blocking**: " + blocking_reason)
    lines.append("- **fingerprint**: " + BT + scope.fingerprint + BT)
    lines.append("- **mode**: " + BT + scope.mode + BT)
    if scope.head:
        lines.append("- **head**: " + BT + scope.head + BT)
    if scope.affected_packages:
        lines.append("- **packages**: " + BT + ", ".join(scope.affected_packages) + BT)
    total_ms = sum(r.duration_ms for r in results)
    lines.append("- **total_time**: {:.1f}s".format(total_ms / 1000))
    lines.append("")

    lines.append("## Results")
    lines.append("")
    lines.append("| stage | check | status | gate | time | message |")
    lines.append("|-------|-------|--------|------|------|---------|")
    for r in results:
        icon = STATUS_ICONS.get(r.status, r.status)
        if r.duration_ms >= 100:
            time_str = "{:.1f}s".format(r.duration_ms / 1000)
        else:
            time_str = "{}ms".format(r.duration_ms)
        msg = r.message.replace("|", "/")[:80]
        lines.append("| {} | {}{}{} | {} | {} | {} | {} |".format(
            r.stage, BT, r.id, BT, icon, r.gate, time_str, msg))
    lines.append("")

    failures = [r for r in results if r.status in ("fail", "error", "timeout")]
    if failures:
        lines.append("## Details")
        lines.append("")
        for r in failures:
            lines.append("### " + BT + r.id + BT + " (" + r.status + ")")
            lines.append("")
            lines.append("- command: " + BT + r.command + BT)
            lines.append("- message: " + r.message)
            lines.append("")
            if r.output:
                lines.append(FENCE)
                lines.append(r.output[:2048])
                lines.append(FENCE)
                lines.append("")

    return NL.join(lines)


def write_report(output_path, scope, results):
    """Write <output>.checks.json and <output>.checks.md."""
    review_allowed, overall, blocking = determine_verdict(results)

    data = {
        "schema": 1,
        "timestamp": datetime.now(timezone.utc).isoformat(),
        "scope": {
            "mode": scope.mode,
            "base": scope.base,
            "head": scope.head,
            "branch": scope.branch,
            "fingerprint": scope.fingerprint,
            "affected_packages": scope.affected_packages,
            "dirty": scope.dirty,
            "all_packages": scope.all_packages,
        },
        "review_allowed": review_allowed,
        "overall": overall,
        "blocking_reason": blocking,
        "results": [r.to_dict() for r in results],
        "total_duration_ms": sum(r.duration_ms for r in results),
    }
    json_path = output_path + ".checks.json"
    with open(json_path, "w", encoding="utf-8", newline=NL) as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
        f.write(NL)

    md_path = output_path + ".checks.md"
    md = render_md(scope, results, review_allowed, overall, blocking)
    with open(md_path, "w", encoding="utf-8", newline=NL) as f:
        f.write(md + NL)

    return json_path, md_path, review_allowed, overall