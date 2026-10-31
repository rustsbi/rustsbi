---
name: rustsbi-code-review
description: Review code against RustSBI's persona-keyed coding guidelines and write a Markdown review file. Use when asked to review a Git change (diff mode) or a set of target files (files mode), or to backtest the skill against a historical pull request.
---

# rustsbi-code-review

Review code against RustSBI's coding guidelines — `docs/src/guidelines/` —
and write one Markdown review file.

The review is **recall-first**: miss as few real defects as possible.
It makes objective calls (bugs) and subjective calls (guideline violations,
each grounded in a cited rule short-name).

## Interface

The skill receives ONE raw argument string beginning with a mode word:

```
diff   <base|pr/N[@rev]>  <output>
files  <path[:lines] ...> <output>
```

- `diff <base>` — the commit series `merge-base(<base>, HEAD)..HEAD`, each commit's message and its diff.
- `diff pr/N` — a historical pull request read through `gh` (no checkout needed). This is the backtest mode.
  The working tree is **not** checked out at that commit, so the pass reviews the diff as given
  and must not treat the current files as the reviewed revision.
- `diff pr/N@rev` — the same pull request **as of `rev`** (`base...rev` through the compare API).
  Pin the revision a review was written against before comparing with it:
  a PR usually carries its own fix commits, so reviewing `pr/N` at head hides exactly the defects
  the maintainers reported.
- `files <path[:lines] ...>` — working-tree contents of the named files, optionally narrowed to line ranges.
- `<output>` — required, last positional: the assembled review file.

Pass the raw argument string **verbatim**, as one quoted argument, to the resolver;
do not pre-split it.

## Guidelines (progressive disclosure)

Three personas. The guideline text lives in the repository — **never copy it into this skill**.

| persona | review section | index |
|---|---|---|
| `maintainability` | `## Maintainability` | `docs/src/guidelines/for-maintainability/README.md` |
| `development` | `## Correctness & Safety` | `docs/src/guidelines/for-development/README.md` |
| `documentation` | `## Documentation` | `docs/src/guidelines/for-documentation/README.md` |

Disclosure has four layers; a pass stops as early as it can:

1. **L0** the table above — always.
2. **L1** the persona's `README.md` index: every rule's short-name and one-line gist.
3. **L2** a topic file in the same directory, opened **only** on a suspected violation.
4. **L3** inside that file, only the anchored rule `### Title (short-name) {#short-name}`.

Hard constraint: a pass reads L1 for every activated persona,
but must **not** read every L2 file up front.

The guidelines root defaults to `docs/src/guidelines`;
override it with `RCR_GUIDELINES_ROOT` when the guidelines live in another worktree or branch
(required when backtesting a PR that predates the guidelines).

## Activation

- `maintainability`, `development` — always.
- `documentation` — only when a reviewed path is under `docs/`, is a `*.md`, or changes a user-facing API surface.

Activation is path-based and deterministic; do not ask a model to triage it.

## Pipeline

Three steps, in order. All tooling lives in one Rust binary,
`scripts/rustsbi-review-cli` (one subcommand per stage); the former Python
scripts under `scripts/*.py` and `scripts/checks/` remain only as a frozen
reference and are not part of this path. External requirements: `git` (and
`gh`, for backtest mode), plus a Rust toolchain — which any machine that can
run the build/test checks already has.

Shells do not persist, so re-derive `SKILL` in every command:

```sh
SKILL="$(git rev-parse --show-toplevel)/.agents/skills/rustsbi-code-review"
```

Resolve the CLI binary (`RCR_CLI`) in every command. This chain is fully
machine-checkable, needs no interpreter probing, and never asks the user:

```sh
RCR_CLI="${RCR_CLI:-$SKILL/scripts/rustsbi-review-cli/target/release/rustsbi-review-cli}"
[ -x "$RCR_CLI" ] || cargo build --release \
  --manifest-path "$SKILL/scripts/rustsbi-review-cli/Cargo.toml" \
  || { echo "rustsbi-review-cli build failed (is cargo on PATH?)" >&2; exit 1; }
```

- `$RCR_CLI`, when the caller sets it, wins;
- otherwise use the release binary inside the skill checkout, building it
  first when missing (idempotent, incremental, ~10s cold);
- on Windows the binary is `rustsbi-review-cli.exe` — append the suffix there.

1. **Resolve the target.**
   `"$RCR_CLI" resolve-target --meta '<raw args>'` prints
   `mode=`, `base=`, `head=`, `branch=`, `date=`.
   The same call without `--meta` writes the canonical review input to `<output>.input.md`.
2. **Review — combined by default, persona fan-out on request.**
   `"$RCR_CLI" run-fanout <output>.input.md <output>.comments.json`
   runs **combined**: one pass, every activated persona index in the same context.
   Add `--mode persona` for one isolated pass per persona, each with only its own
   L1 index (selective exposure) but the same whole change, run in parallel.

   Measured on `pr/220@354248d`: combined reproduced 8 of 8 ground-truth comments in
   its best run at 0.54M read tokens; persona fan-out reproduced 2 of 8 at 4.10M read
   tokens. Persona mode buys independent opinions, not coverage, and costs 7.6
   times the read tokens. Both modes vary widely run to run.

   Neither mode slices by file or line count: slicing one opinion across files produced
   eight separate comments for a single cross-cutting defect, which no reader can reassemble.

   For one persona, or a single-file target, call a pass directly:
   `"$RCR_CLI" run-pass <output>.input.md <output>.comments.json --persona development`
   composes the prompt in the fixed order
   (`pass_contract.md` → the activated personas' L1 `README.md` → the review input),
   hands it to a headless agent on **stdin** (a command line is capped near 32k characters),
   and validates the JSON array that comes back.

   It drives `claude -p --allowedTools Read,Write --permission-mode acceptEdits` by default;
   set `RCR_AGENT_CMD` for a different agent, and pass `--persona NAME` to narrow activation.

   Grant the pass Read/Write explicitly. An agent spawned with tools denied cannot return its
   comments, and it stalls retrying instead of failing loudly.

   **Cost is measured, not self-reported.** Each pass writes
   `<comments.json>.usage.json` from the agent CLI's own envelope
   (`input_tokens`, `output_tokens`, `cache_read_input_tokens`, `total_cost_usd`, API time),
   and `run-fanout` sums the passes into one sidecar.
   `"$RCR_CLI" usage <path-or-glob>` prints the table with
   values scaled to `k` / `M` and two decimals; the sidecar keeps raw integers.
   A pass costs roughly 25k input tokens before the review input is even counted
   (system prompt and tool schema), so measure before adding passes.
3. **Assemble, then archive.**
   `"$RCR_CLI" assemble <meta> <comments.json> <output>`
   groups by persona, sorts by file then line, drops exact duplicates,
   writes the frontmatter, and leaves a `<!-- SUMMARY -->` placeholder.
   Then `"$RCR_CLI" archive <output>` snapshots every file
   of this run into `<run-dir>/<YYYYMMDD-HHMMSS>/`. The files directly under the
   run directory stay as the overwritable **latest** copy, so they and the newest
   timestamped subdirectory are always identical; history accumulates in the stamps.

Do not spawn a pass by re-running this skill:
a pass is a reviewer whose input is the prompt from step 2, not another orchestration.

## Output format

```markdown
---
date: 2026-10-30
mode: diff
base: pr/412
head: 1c1b069
branch: refactor/dynamic-hart-state
---

<!-- SUMMARY -->

## Maintainability

### `path/to/file.rs` line 42

> ```diff
> +    let n = ctx.len();
> ```

`descriptive-names` (nit) — `Be descriptive`: `n` is opaque at the point of use.

**Fix.** Rename it to `hart_count`.
```

A comment that cites a rule carries that rule's one-line gist after the severity,
so a reader can check the basis without opening the guideline.
Prose groundings (`Reachable panic`) are for defects no rule names.

Sections appear in the fixed order Maintainability, Correctness & Safety, Documentation.
Leave the `<!-- SUMMARY -->` placeholder untouched.

## Backtest

`diff pr/N` reads a historical PR through `gh`; set `RCR_GUIDELINES_ROOT` to the
guidelines checkout, since a PR that old predates `docs/src/guidelines/` in its own tree.

`cases/` holds the real review comments of a PR (the positive test set), and
`score.py <case-file> <comments.json>` (still Python; scoring only, outside the
pipeline) reports recall and precision against them. Neither is part of the
pipeline above; they measure it.
