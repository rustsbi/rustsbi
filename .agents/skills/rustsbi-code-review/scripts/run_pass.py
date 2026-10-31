#!/usr/bin/env python3
"""run_pass.py - spawn one review pass and collect its comments and cost.

Composes the pass prompt deterministically, in the cache-stable order:

    pass_contract.md  ->  the activated personas' L1 index  ->  the review input

and hands it to a headless agent on stdin. Stdin, not argv: Windows caps a
command line at about 32k characters, and a real review input is far larger.

    run_pass.py <review-input> <comments.json> [--persona NAME ...]

Token accounting is external, never self-reported. For the claude agent the
command runs with --output-format json and the envelope's usage block
(input/output/cache tokens, total_cost_usd, api duration) is written to
<comments.json>.usage.json next to the comments.

Agents:
    claude (default)   claude -p --output-format json --allowedTools Read,Write
                       --permission-mode acceptEdits
    anything else      set RCR_AGENT_CMD to a command that reads the prompt on
                       stdin and writes the answer to stdout (no usage capture).
"""
import json
import os
import re
import subprocess
import sys

NL = chr(10)
HERE = os.path.dirname(os.path.abspath(__file__))
SKILL = os.path.dirname(HERE)
PERSONAS = [
    ("maintainability", "docs/src/guidelines/for-maintainability/README.md"),
    ("development", "docs/src/guidelines/for-development/README.md"),
    ("documentation", "docs/src/guidelines/for-documentation/README.md"),
]
NOISE = re.compile(r"^\[claude-code:[^\]]*\]")


def die(msg):
    sys.stderr.write(msg.rstrip() + NL)
    sys.exit(2)


def read(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        return f.read()


def build_prompt(input_path, personas):
    root = os.environ.get("RCR_GUIDELINES_ROOT", "docs/src/guidelines")
    parts = [read(os.path.join(SKILL, "pass_contract.md")), NL, "---", NL]
    for name, rel in personas:
        index = os.path.join(root, rel.split("docs/src/guidelines/", 1)[-1])
        parts += ["# Guideline index: {}".format(name), NL, read(index), NL]
    parts += ["---",
              "Guideline root: " + os.path.abspath(root),
              "Links in an index are relative to its own persona directory.",
              "Open a topic file only on a suspected violation, and read only that rule's section.",
              "",
              "The working tree is NOT checked out at the reviewed revision.",
              "Review the REVIEW INPUT as given; do not run git commands.",
              "", read(input_path)]
    return NL.join(parts)


def strip_noise(text):
    return NL.join(l for l in text.splitlines() if not NOISE.match(l))


def extract_json(text):
    text = strip_noise(text)
    start = text.find("[")
    end = text.rfind("]")
    if start < 0 or end < start:
        return None
    return text[start:end + 1]


def run_agent(prompt, agent):
    """Return (answer text, usage dict or None)."""
    if agent == "claude":
        cmd = ["claude", "-p", "--output-format", "json",
               "--allowedTools", "Read,Write", "--permission-mode", "acceptEdits"]
        # Pin a model per pass with RCR_MODEL (an alias such as haiku/opus/sonnet,
        # resolved through the ANTHROPIC_DEFAULT_*_MODEL map, or a full model name).
        if os.environ.get("RCR_MODEL"):
            cmd += ["--model", os.environ["RCR_MODEL"]]
    elif os.environ.get("RCR_AGENT_CMD"):
        cmd = os.environ["RCR_AGENT_CMD"].split()
    else:
        die("error: no agent command; set RCR_AGENT_CMD or use --agent claude")

    p = subprocess.run(cmd, input=prompt, capture_output=True, text=True,
                       encoding="utf-8", errors="replace")
    if p.returncode != 0:
        die("error: " + " ".join(cmd) + NL + (p.stderr or "").strip())

    out = strip_noise(p.stdout).strip()
    if agent != "claude":
        return out, None
    try:
        env = json.loads(out)
    except ValueError:
        return out, None
    usage = {
        "total_cost_usd": env.get("total_cost_usd"),
        "duration_api_ms": env.get("duration_api_ms"),
        "num_turns": env.get("num_turns"),
        "session_id": env.get("session_id"),
        "usage": env.get("usage"),
        "model_usage": env.get("modelUsage"),
    }
    return env.get("result") or "", usage


def main():
    argv = sys.argv[1:]
    agent = "claude"
    wanted = []
    rest = []
    i = 0
    while i < len(argv):
        if argv[i] == "--agent":
            agent = argv[i + 1]
            i += 2
        elif argv[i] == "--persona":
            wanted.append(argv[i + 1])
            i += 2
        else:
            rest.append(argv[i])
            i += 1
    if len(rest) != 2:
        die("usage: run_pass.py <review-input> <comments.json> [--persona NAME ...]")

    personas = [p for p in PERSONAS if not wanted or p[0] in wanted]
    prompt = build_prompt(rest[0], personas)
    answer, usage = run_agent(prompt, agent)

    body = extract_json(answer)
    if body is None:
        with open(rest[1] + ".raw", "w", encoding="utf-8", newline=NL) as f:
            f.write(answer)
        die("error: the pass returned no JSON array; raw output in " + rest[1] + ".raw")
    try:
        comments = json.loads(body)
    except ValueError as exc:
        with open(rest[1] + ".raw", "w", encoding="utf-8", newline=NL) as f:
            f.write(answer)
        die("error: invalid JSON: " + str(exc) + "; raw output in " + rest[1] + ".raw")

    with open(rest[1], "w", encoding="utf-8", newline=NL) as f:
        json.dump(comments, f, indent=2, ensure_ascii=False)
        f.write(NL)

    if usage is not None:
        usage["personas"] = [p[0] for p in personas]
        with open(rest[1] + ".usage.json", "w", encoding="utf-8", newline=NL) as f:
            json.dump(usage, f, indent=2)
            f.write(NL)
        u = usage.get("usage") or {}
        print("{}: {} comments  |  in={} out={} cache_read={} cost=${:.4f}".format(
            rest[1], len(comments), u.get("input_tokens"), u.get("output_tokens"),
            u.get("cache_read_input_tokens"), usage.get("total_cost_usd") or 0.0))
    else:
        print("{}: {} comments".format(rest[1], len(comments)))


if __name__ == "__main__":
    main()
