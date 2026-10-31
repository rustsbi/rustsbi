#!/usr/bin/env python3
"""assemble_review.py - deterministic merge of one pass's comments into the review file.

No judgement here: group by persona in a fixed order, sort by file then line,
drop exact duplicates, write the frontmatter, and leave a SUMMARY placeholder.

    assemble_review.py <meta-file> <comments.json> <output-file>

The meta file holds key=value lines: mode, base or files, head, branch, date.
"""
import json
import os
import re
import sys

NL = chr(10)
BT = chr(96)
FENCE = BT * 3
KEBAB = re.compile(r"[a-z0-9]+(-[a-z0-9]+)*")

ORDER = [
    ("maintainability", "Maintainability"),
    ("development", "Correctness & Safety"),
    ("documentation", "Documentation"),
]


def die(msg):
    sys.stderr.write(msg.rstrip() + NL)
    sys.exit(2)


def grounding_tag(g):
    g = g or ""
    return BT + g + BT if KEBAB.fullmatch(g) else g


def load_meta(path):
    meta = {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            line = line.strip()
            if "=" in line:
                k, v = line.split("=", 1)
                meta[k.strip()] = v.strip()
    return meta


def render_comment(c):
    file_ = (c.get("file") or "").strip()
    line = c.get("line")
    out = []
    if line is None:
        out.append("### " + BT + file_ + BT)
    else:
        out.append("### " + BT + file_ + BT + " line " + str(line))
    out.append("")
    snippet = (c.get("diff") or "").rstrip()
    if snippet:
        out.append("> " + FENCE + "diff")
        for ln in snippet.splitlines():
            out.append("> " + ln)
        out.append("> " + FENCE)
        out.append("")
    gist = (c.get("rule_gist") or "").strip()
    header = "{} ({})".format(grounding_tag(c.get("grounding")),
                              c.get("severity") or "minor")
    if gist:
        # The gist may itself contain backticks (a rule title often does), so it is
        # emphasised rather than wrapped in a code span, which would not nest.
        header += " — *" + gist + "*"
    out.append("{}: {}".format(header, (c.get("problem") or "").strip()))
    out.append("")
    out.append("**Fix.** " + (c.get("fix") or "").strip())
    out.append("")
    return NL.join(out)


def main():
    if len(sys.argv) != 4:
        die("usage: assemble_review.py <meta-file> <comments.json> <output-file>")
    meta_path, frag_path, out_path = sys.argv[1], sys.argv[2], sys.argv[3]
    if os.path.exists(out_path):
        die("error: refusing to overwrite " + out_path)

    meta = load_meta(meta_path)
    with open(frag_path, encoding="utf-8") as f:
        raw = f.read().strip()
    if raw.startswith(FENCE):
        raw = raw.split(NL, 1)[1].rsplit(FENCE, 1)[0]
    comments = json.loads(raw or "[]")
    if isinstance(comments, dict):
        comments = comments.get("comments") or []

    lines = ["---"]
    for key in ("date", "mode", "base", "files", "head", "branch", "title"):
        if meta.get(key):
            lines.append("{}: {}".format(key, meta[key]))
    lines += ["---", "", "<!-- SUMMARY -->", ""]

    kept = 0
    for persona, section in ORDER:
        group = [c for c in comments if c.get("persona") == persona]
        seen, unique = set(), []
        for c in group:
            key = (c.get("file"), c.get("line"), c.get("grounding"), c.get("problem"))
            if key not in seen:
                seen.add(key)
                unique.append(c)
        unique.sort(key=lambda c: (c.get("file") or "", c.get("line") or 0))
        if not unique:
            continue
        lines += ["## " + section, ""]
        for c in unique:
            lines.append(render_comment(c))
        kept += len(unique)
    while lines and lines[-1] == "":
        lines.pop()
    with open(out_path, "w", encoding="utf-8", newline=NL) as f:
        f.write(NL.join(lines) + NL)
    print("{}: {} comments ({} in, {} dropped as duplicates)".format(
        out_path, kept, len(comments), len(comments) - kept))


if __name__ == "__main__":
    main()
