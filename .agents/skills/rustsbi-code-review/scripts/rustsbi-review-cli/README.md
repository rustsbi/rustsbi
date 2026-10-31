# rustsbi-review-cli

The complete Rust script layer of the `rustsbi-code-review` skill: the
deterministic check pipeline plus the LLM review orchestration that used to
live in `scripts/*.py`. One binary, one subcommand per former script.

| subcommand | replaces | role |
|---|---|---|
| `checks` | `checks/cli.py` | scope → convention → build → lint → test → report |
| `resolve-target` | `resolve_target.py` | build the canonical review input (`--meta` for metadata) |
| `run-pass` | `run_pass.py` | compose the pass prompt, spawn the agent, collect comments + usage |
| `run-fanout` | `run_fanout.py` | combined or per-persona passes, merge + dedup, sum usage |
| `assemble` | `assemble_review.py` | merge comments into the review file |
| `archive` | `archive_run.py` | snapshot a run into a timestamped subdirectory |
| `usage` | `usage.py` | token accounting table over `.usage.json` sidecars |

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
`RCR_AGENT_CMD` — same meanings as in the Python versions.

## Parity

Every subcommand is a behavioral port verified byte-for-byte against its
Python counterpart on the same inputs (see `.review-out/parity/run.sh`:
resolve-target md/meta, run-pass comments/usage/stdout via a stub `claude`,
run-fanout merge + usage sidecar, assemble output, archive listing, usage
table). Known intentional deviations:

- `checks` maps subprocess timeouts to `status: "timeout"` instead of the
  Python crash;
- `command` labels for internal checks are `internal: ...` in both versions
  (changed from `python: ...` during the parity diff);
- the scope `fingerprint` hash input differs (cache key, never consumed
  across versions).

The Python scripts remain in place until the skill is switched over;
`score.py` (backtest scoring, outside the pipeline) is still Python.