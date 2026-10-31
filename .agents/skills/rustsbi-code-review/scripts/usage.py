#!/usr/bin/env python3
"""usage.py - token accounting across review runs.

Only tokens are recorded, never a price: the agent CLI's own envelope supplies
them, so nothing here depends on the model reporting its usage.
The sidecar files keep raw integers; this report prints them scaled to k/M.

Four counters, per pass and summed:

    cache write   cache_creation_input_tokens   prompt tokens written to the cache
    cache read    cache_read_input_tokens       prompt tokens served from the cache
    plain read    input_tokens                  prompt tokens sent uncached
    write         output_tokens                 generated tokens

    total read  = cache write + cache read + plain read
    total write = write

    usage.py <path-or-glob> [...]

Reads the .usage.json sidecars written by run_pass.py and run_fanout.py.
"""
import glob
import json
import sys

NL = chr(10)
FIELDS = [
    ("cache_write", "cache_creation_input_tokens"),
    ("cache_read", "cache_read_input_tokens"),
    ("plain_read", "input_tokens"),
]


def human(n):
    """1234567 -> '1.23M'. Raw integers stay in the sidecar files."""
    n = int(n or 0)
    if abs(n) >= 1000000:
        return "{:.2f}M".format(n / 1000000.0)
    if abs(n) >= 1000:
        return "{:.2f}k".format(n / 1000.0)
    return str(n)


def load(pattern):
    paths = glob.glob(pattern, recursive=True) or glob.glob(pattern + "*")
    out = []
    for p in sorted(paths):
        if not p.endswith(".usage.json"):
            continue
        with open(p, encoding="utf-8") as f:
            out.append((p, json.load(f)))
    return out


def counters(usage):
    u = usage or {}
    row = {name: u.get(key) or 0 for name, key in FIELDS}
    row["write"] = u.get("output_tokens") or 0
    row["total_read"] = row["cache_write"] + row["cache_read"] + row["plain_read"]
    return row


def passes(path, u):
    if "passes" in u:
        for p in u["passes"]:
            yield (path, p.get("persona") or "-", counters(p.get("usage")),
                   u.get("mode") or "-")
    else:
        yield (path, ",".join(u.get("personas") or ["-"]) or "-",
               counters(u.get("usage")), u.get("mode") or "combined")


def main():
    if len(sys.argv) < 2:
        sys.exit("usage: usage.py <path-or-glob> [...]")
    rows = []
    for pattern in sys.argv[1:]:
        for path, u in load(pattern):
            rows += list(passes(path, u))
    if not rows:
        sys.exit("no .usage.json found")

    head = "{:<34} {:<8} {:<18} {:>10} {:>10} {:>10} {:>11} {:>9}"
    print(head.format("run", "mode", "persona", "cache_wr", "cache_rd",
                      "plain_rd", "total_read", "write"))
    for path, persona, c, mode in rows:
        print(head.format(path[-34:], mode, persona, human(c["cache_write"]),
                          human(c["cache_read"]), human(c["plain_read"]),
                          human(c["total_read"]), human(c["write"])))
    agg = {k: sum(c[k] for _, _, c, _ in rows)
           for k in ("cache_write", "cache_read", "plain_read", "total_read", "write")}
    print(head.format("TOTAL ({} passes)".format(len(rows)), "", "",
                      human(agg["cache_write"]), human(agg["cache_read"]),
                      human(agg["plain_read"]), human(agg["total_read"]),
                      human(agg["write"])))


if __name__ == "__main__":
    main()
