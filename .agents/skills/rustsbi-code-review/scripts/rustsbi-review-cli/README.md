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

## Build

```sh
cargo build --release
# binary: target/release/rustsbi-review-cli
```

Any machine that can run the pipeline already has a Rust toolchain (the
check stages invoke cargo), so this adds no new prerequisite.

## Environment

`RCR_SKILL_HOME` (skill dir holding `pass_contract.md`; auto-detected by
walking up from the binary), `RCR_GUIDELINES_ROOT`, `RCR_REPO`, `RCR_MODEL`,
`RCR_AGENT_CMD`.

## Scoring

`score.py` (backtest scoring, outside the pipeline) is still Python: it
compares a generated review against the ground-truth case files in
`cases/` and reports recall and precision.
