# rustsbi-review-cli

The complete Rust script layer of the `rustsbi-code-review` skill: the
deterministic check pipeline plus the LLM review orchestration. One binary,
one subcommand per pipeline stage.

| subcommand | role |
|---|---|
| `checks` | scope → convention → build → lint → test → report |
| `resolve-target` | build the canonical review input (`--meta` for metadata) |
| `run-pass` | compose the pass prompt, spawn the agent, collect comments + usage |
| `run-fanout` | combined or per-persona passes, merge + dedup, sum usage |
| `assemble` | merge comments into the review file |
| `archive` | snapshot a run into a timestamped subdirectory |
| `usage` | token accounting table over `.usage.json` sidecars |

Run `checks '<raw args>' <output>` before review. A compile failure blocks
review; advisory findings remain available to the model. `run-pass` and
`run-fanout` require `--checks <output>.checks.json`, validate its verdict,
and include its results in each review prompt. For checks-only requests,
run `checks` and return its exit code and report paths without a model pass.

## Build

```sh
cargo build --release
# binary: target/release/rustsbi-review-cli
```

Any machine that can run the pipeline already has a Rust toolchain (the
check stages invoke cargo), so this adds no new prerequisite.

The `Skills` workflow discovers Cargo workspaces under `.agents/skills/`
and runs rustfmt, Clippy, builds, and tests for each one. Keep a CLI's
workspace inside its skill directory and commit its `Cargo.lock`.
Workspace members are checked together; new skill CLIs need no workflow entry.

## Environment

`RCR_SKILL_HOME` (skill dir holding `pass_contract.md`; auto-detected by
walking up from the binary), `RCR_GUIDELINES_ROOT`, `RCR_REPO`, `RCR_MODEL`,
`RCR_AGENT_CMD`.

## Scoring

`rustsbi-review-cli score CASE COMMENTS.json` compares generated comments with historical anchors.
