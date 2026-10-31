#!/usr/bin/env python3
"""resolve_target.py - build the canonical review input for rustsbi-code-review.

The skill is triggered from a prompt, not a shell, so the raw argument string
arrives with its quotes intact; this script owns the tokenization.

    resolve_target.py '<raw args>'          # review input -> <output>.input.md
    resolve_target.py --meta '<raw args>'   # meta key=value lines -> stdout

Raw args:
    diff   <base|pr/N>        <output>
    files  <path[:lines] ...> <output>

    diff <base>   merge-base(<base>, HEAD)..HEAD, each commit's message and diff
    diff pr/N     a historical pull request, read through gh (no checkout needed)
    files <t>     working-tree contents of the named paths
"""
import datetime
import json
import os
import re
import subprocess
import sys

NL = chr(10)
BT = chr(96)
FENCE = BT * 3
RANGE = re.compile(r"^[0-9]+(-[0-9]+)?(,[0-9]+(-[0-9]+)?)*$")
PR_REF = re.compile(r"^(?:pr/|pull/)([0-9]+)(?:/head)?(?:@(.+))?")
REV = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._/-]*$")
LANG = {"rs": "rust", "md": "markdown", "py": "python", "sh": "sh",
        "toml": "toml", "yaml": "yaml", "yml": "yaml", "json": "json"}


def die(msg):
    sys.stderr.write(msg.rstrip() + NL)
    sys.exit(2)


def run(args):
    # Pin UTF-8: a Windows console locale (e.g. GBK) cannot decode diff and
    # commit text that holds non-ASCII characters, and would yield stdout=None.
    p = subprocess.run(args, capture_output=True, text=True,
                       encoding="utf-8", errors="replace")
    if p.returncode != 0:
        err = (p.stderr or p.stdout).strip()
        die("error: " + " ".join(args) + NL + err)
    return p.stdout


def tokenize(raw):
    """Whitespace split, except inside double quotes."""
    toks, cur, quoted = [], [], False
    for ch in raw:
        if ch == '"':
            quoted = not quoted
        elif ch.isspace() and not quoted:
            if cur:
                toks.append("".join(cur))
                cur = []
        else:
            cur.append(ch)
    if cur:
        toks.append("".join(cur))
    return toks


def parse_ranges(spec):
    out = []
    for part in spec.split(","):
        if "-" in part:
            a, b = part.split("-", 1)
            out.append((int(a), int(b)))
        else:
            out.append((int(part), int(part)))
    return out


def split_target(tok):
    if ":" in tok:
        path, _, spec = tok.rpartition(":")
        if path and RANGE.match(spec):
            return path, parse_ranges(spec)
    return tok, None


def repo_slug():
    if os.environ.get("RCR_REPO"):
        return os.environ["RCR_REPO"]
    return run(["gh", "repo", "view", "--json", "nameWithOwner",
                "-q", ".nameWithOwner"]).strip()


def render_patch(files):
    """Rebuild a git-style patch from the compare API's per-file patches."""
    out = []
    for f in files:
        body = (f.get("patch") or "").rstrip(NL)
        if not body:
            continue
        out.append("diff --git a/{0} b/{0}".format(f["filename"]))
        out.append(body)
    return NL.join(out)


def render_pr(num, rev=None):
    repo = repo_slug()
    info = json.loads(run(["gh", "pr", "view", str(num), "--repo", repo, "--json",
                           "number,title,baseRefName,baseRefOid,headRefOid,"
                           "headRefName,commits"]))
    if rev:
        # Pin a historical revision: review base...rev instead of the PR head.
        # The ground truth of a review was written against an earlier revision,
        # and the PR's own fix commits would otherwise hide it.
        if not REV.match(rev):
            die("error: unusable revision: {}".format(rev))
        base = info.get("baseRefOid") or ""
        cmp_data = json.loads(run(["gh", "api", "repos/{}/compare/{}...{}".format(
            repo, base, rev)]))
        commits = [{"oid": c.get("sha", ""),
                    "messageHeadline": (c.get("commit", {}).get("message") or "").split(NL)[0],
                    "messageBody": NL.join(
                        (c.get("commit", {}).get("message") or "").split(NL)[1:]).strip()}
                   for c in (cmp_data.get("commits") or [])]
        patch = render_patch(cmp_data.get("files") or [])
        head = rev[:7]
        source = "pr/{}@{}".format(num, rev)
    else:
        commits = info.get("commits") or []
        patch = run(["gh", "pr", "diff", str(num), "--repo", repo])
        head = (info.get("headRefOid") or "")[:7]
        source = "pr/{}".format(num)

    lines = ["# REVIEW INPUT", "",
             "mode: diff",
             "source: {}".format(source),
             "title: {}".format((info.get("title") or "").strip()),
             "base: {} ({})".format(info.get("baseRefName"),
                                    (info.get("baseRefOid") or "")[:8]),
             "head: {}".format(head),
             "branch: {}".format(info.get("headRefName") or ""),
             "", "## Commits, oldest first", ""]
    for c in commits:
        lines.append("### commit {}".format(c["oid"][:7]))
        lines.append((c.get("messageHeadline") or "").strip())
        body = (c.get("messageBody") or "").strip()
        if body:
            lines += ["", body]
        lines.append("")
    lines += ["## Diff", "", FENCE + "diff", patch.rstrip(NL), FENCE, ""]

    meta = ["mode=diff", "base={}".format(source),
            "head={}".format(head),
            "branch={}".format(info.get("headRefName") or ""),
            "date={}".format(datetime.date.today().isoformat())]
    return NL.join(lines), meta


def render_git(base):
    head = run(["git", "rev-parse", "--short", "HEAD"]).strip()
    branch = run(["git", "rev-parse", "--abbrev-ref", "HEAD"]).strip()
    mb = run(["git", "merge-base", base, "HEAD"]).strip()
    log = run(["git", "log", "-p", "--reverse",
               "--format=### commit %h%n%s%n%b", "{}..HEAD".format(mb)])
    lines = ["# REVIEW INPUT", "",
             "mode: diff",
             "base: {} ({})".format(base, mb[:8]),
             "head: {}".format(head),
             "branch: {}".format(branch),
             "", FENCE + "diff", log.rstrip(NL), FENCE, ""]
    meta = ["mode=diff", "base={}".format(base), "head={}".format(head),
            "branch={}".format(branch),
            "date={}".format(datetime.date.today().isoformat())]
    return NL.join(lines), meta


def render_files(targets):
    out = ["# REVIEW INPUT", "", "mode: files", ""]
    for tok in targets:
        path, ranges = split_target(tok)
        if not os.path.isfile(path):
            die("error: no such file: {}".format(path))
        with open(path, encoding="utf-8", errors="replace") as f:
            body = f.read().splitlines()
        keep = set()
        if ranges:
            for a, b in ranges:
                keep.update(range(a, b + 1))
        else:
            keep.update(range(1, len(body) + 1))
        label = path
        if ranges:
            label += ":" + ",".join(
                "{}-{}".format(r[0], r[1]) if r[0] != r[1] else "{}".format(r[0])
                for r in ranges)
        lang = LANG.get(path.rsplit(".", 1)[-1].lower(), "") if "." in path else ""
        out += ["## " + BT + label + BT, "", FENCE + lang]
        for i, text in enumerate(body, 1):
            if i in keep:
                out.append("{:5d} | {}".format(i, text))
        out += [FENCE, ""]
    meta = ["mode=files",
            "files={}".format(" ".join(targets)),
            "date={}".format(datetime.date.today().isoformat())]
    return NL.join(out), meta


def main():
    argv = sys.argv[1:]
    meta_only = False
    if argv and argv[0] == "--meta":
        meta_only = True
        argv = argv[1:]
    raw = argv[0] if argv else ""
    toks = tokenize(raw)
    if len(toks) < 3:
        die("usage: diff <base|pr/N> <output> | files <path[:lines]> ... <output>")

    mode, output = toks[0], toks[-1]
    targets = toks[1:-1]

    if mode == "diff":
        m = PR_REF.match(targets[0])
        text, meta = render_pr(*m.groups()) if m else render_git(targets[0])
    elif mode == "files":
        text, meta = render_files(targets)
    else:
        die("error: unknown mode: {}".format(mode))

    if meta_only:
        print(NL.join(meta + ["output={}".format(output)]))
        return
    dest = output + ".input.md"
    with open(dest, "w", encoding="utf-8", newline=NL) as f:
        f.write(text)
    print(dest)


if __name__ == "__main__":
    main()
