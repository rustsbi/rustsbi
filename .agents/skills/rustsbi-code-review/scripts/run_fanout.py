#!/usr/bin/env python3
"""run_fanout.py - run a review in one of two modes.

    run_fanout.py <review-input> <comments.json> [--mode combined|persona] [--persona NAME ...]

combined (default)
    ONE pass over the whole change with every activated persona index in the same
    context. Cheap, and the personas cross-reference: a naming defect in one file
    and an RV64 gating defect in another are seen by the same reader.

persona
    One pass per persona, each in its own context with only its own L1 index
    (selective exposure), all against the whole change. Three times the tokens
    before the review input is counted, and measured on pr/220 it reproduced
    fewer real review comments than combined.

Either way the passes run as child processes, the merged list is de-duplicated,
and the token usage is summed into <comments.json>.usage.json.
"""
import json
import os
import subprocess
import sys

NL = chr(10)
HERE = os.path.dirname(os.path.abspath(__file__))
RUN_PASS = os.path.join(HERE, "run_pass.py")
DEFAULT = ["maintainability", "development", "documentation"]


def die(msg):
    sys.stderr.write(msg.rstrip() + NL)
    sys.exit(2)


def spawn(src, dest, personas):
    cmd = [sys.executable, RUN_PASS, src, dest]
    for p in personas:
        cmd += ["--persona", p]
    return subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, encoding="utf-8", errors="replace")


def main():
    argv = sys.argv[1:]
    mode = "combined"
    personas = []
    rest = []
    i = 0
    while i < len(argv):
        if argv[i] == "--mode":
            mode = argv[i + 1]
            i += 2
        elif argv[i] == "--persona":
            personas.append(argv[i + 1])
            i += 2
        else:
            rest.append(argv[i])
            i += 1
    if len(rest) != 2:
        die("usage: run_fanout.py <review-input> <comments.json> [--mode combined|persona] [--persona NAME ...]")
    if mode not in ("combined", "persona"):
        die("error: --mode must be combined or persona")
    src, out = rest
    personas = personas or DEFAULT

    jobs = []
    if mode == "combined":
        dest = out + ".combined.json"
        jobs.append((",".join(personas), dest, spawn(src, dest, personas)))
    else:
        for name in personas:
            dest = "{}.{}.json".format(out, name)
            jobs.append((name, dest, spawn(src, dest, [name])))

    merged, failed, passes = [], [], []
    totals = {"cache_creation_input_tokens": 0, "cache_read_input_tokens": 0,
              "input_tokens": 0, "output_tokens": 0}
    for name, dest, proc in jobs:
        stdout, stderr = proc.communicate()
        if proc.returncode != 0 or not os.path.exists(dest):
            failed.append((name, (stderr or stdout or "").strip()[-300:]))
            continue
        with open(dest, encoding="utf-8") as f:
            got = json.load(f)
        print("  {}: {} comments".format(name, len(got)))
        merged += got
        upath = dest + ".usage.json"
        if os.path.exists(upath):
            with open(upath, encoding="utf-8") as f:
                u = json.load(f)
            passes.append({"persona": name, "usage": u.get("usage"),
                           "duration_api_ms": u.get("duration_api_ms"),
                           "num_turns": u.get("num_turns")})
            for k in totals:
                totals[k] += (u.get("usage") or {}).get(k) or 0

    if failed:
        for name, err in failed:
            sys.stderr.write("pass {} failed: {}".format(name, err) + NL)

    seen, unique = set(), []
    for c in merged:
        key = (c.get("file"), c.get("line"), c.get("grounding"), (c.get("problem") or "")[:80])
        if key not in seen:
            seen.add(key)
            unique.append(c)
    with open(out, "w", encoding="utf-8", newline=NL) as f:
        json.dump(unique, f, indent=2, ensure_ascii=False)
        f.write(NL)

    if passes:
        with open(out + ".usage.json", "w", encoding="utf-8", newline=NL) as f:
            json.dump({"mode": mode, "passes": passes, "totals": totals}, f, indent=2)
            f.write(NL)
        print("tokens: cache_wr={} cache_rd={} plain_rd={} write={}".format(
            totals["cache_creation_input_tokens"], totals["cache_read_input_tokens"],
            totals["input_tokens"], totals["output_tokens"]))

    print("{}: {} comments from {} pass(es), mode={} ({} duplicates merged)".format(
        out, len(unique), len(jobs), mode, len(merged) - len(unique)))
    if failed:
        sys.exit(1)


if __name__ == "__main__":
    main()
