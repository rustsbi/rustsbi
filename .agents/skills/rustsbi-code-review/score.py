#!/usr/bin/env python3
"""score.py - compare a review against its ground-truth case.

    score.py [--tol N] <case-file> <comments.json>

A ground-truth comment matches a produced comment when the file is the same
(suffix match) and the anchor lines are within --tol (default 5).
Only ground-truth comments tagged [code-derivable] enter the denominator;
untagged comments are assumed code-derivable.

Prints recall (share of ground truth reproduced) and precision (share of
produced comments that hit a ground-truth comment). Both matter: recall can
be gamed by emitting many comments.
"""
import json
import re
import sys

NL = chr(10)
HEADING = re.compile(r"^### " + chr(96) + "(.+?)" + chr(96) + r"(?: line ([0-9]+))?\s*$")
KEBAB = re.compile(r"[a-z0-9]+(-[a-z0-9]+)*")


def parse_case(path):
    gt, cur = [], None
    with open(path, encoding="utf-8") as f:
        for line in f.read().splitlines():
            m = HEADING.match(line)
            if m:
                cur = {"file": m.group(1), "line": int(m.group(2)) if m.group(2) else None,
                       "text": []}
                gt.append(cur)
            elif line.startswith(">") and cur is not None:
                cur["text"].append(line[1:].strip())
    for g in gt:
        text = " ".join(g["text"])
        tag = re.match(r"^\[([a-z-]+)\]", text)
        g["cls"] = tag.group(1) if tag else "code-derivable"
    return gt


def norm(p):
    return p.strip().lstrip("./").replace("\\", "/")


def same_file(a, b):
    a, b = norm(a), norm(b)
    return a == b or a.endswith("/" + b) or b.endswith("/" + a)


def main():
    argv = sys.argv[1:]
    tol = 5
    if argv and argv[0] == "--tol":
        tol = int(argv[1])
        argv = argv[2:]
    if len(argv) != 2:
        sys.exit("usage: score.py [--tol N] <case-file> <comments.json>")
    gt = parse_case(argv[0])
    with open(argv[1], encoding="utf-8") as f:
        produced = json.load(f)
    if isinstance(produced, dict):
        produced = produced.get("comments") or []

    scored = [g for g in gt if g["cls"] == "code-derivable"]
    hit_gt, hit_prod = set(), set()
    for i, g in enumerate(scored):
        for j, p in enumerate(produced):
            if j in hit_prod or not same_file(g["file"], p.get("file") or ""):
                continue
            if g["line"] is not None and p.get("line") is not None:
                if abs(int(g["line"]) - int(p["line"])) > tol:
                    continue
            hit_gt.add(i)
            hit_prod.add(j)

    print("case: {}".format(argv[0]))
    print("ground truth: {} total, {} code-derivable, {} other".format(
        len(gt), len(scored), len(gt) - len(scored)))
    print("produced:     {}".format(len(produced)))
    print("matched:      {}".format(len(hit_gt)))
    recall = len(hit_gt) / len(scored) if scored else 0.0
    precision = len(hit_prod) / len(produced) if produced else 0.0
    print("recall:       {:.1%}  ({} of {})".format(recall, len(hit_gt), len(scored)))
    print("precision:    {:.1%}  ({} of {})".format(precision, len(hit_prod), len(produced)))

    missed = [g for i, g in enumerate(scored) if i not in hit_gt]
    if missed:
        print(NL + "not reproduced:")
        for g in missed:
            print("  {} line {}".format(g["file"], g["line"]))
    cited = [p for p in produced if KEBAB.fullmatch(p.get("grounding") or "")]
    with_gist = [p for p in cited if (p.get("rule_gist") or "").strip()]
    print(NL + "grounding:  {} rule-cited ({} with a quoted gist), {} prose".format(
        len(cited), len(with_gist), len(produced) - len(cited)))
    if cited:
        names = sorted({p.get("grounding") for p in cited})
        print("rules cited: {}".format(", ".join(names)))

    extra = [p for j, p in enumerate(produced) if j not in hit_prod]
    if extra:
        print(NL + "produced but not in ground truth:")
        for p in extra:
            print("  {} line {} [{}]".format(p.get("file"), p.get("line"),
                                              p.get("grounding")))


if __name__ == "__main__":
    main()
