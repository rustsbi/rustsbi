# Independent Pi acceptance — 2026-11-13

## Scenario

Run the installed skill through Pi in an isolated Linux Git repository.
The requested change modifies only `src/lib.rs`; the checkout also contains
unrelated `README.md` edits and an untracked `.cache/session.tmp` file.
Ask Pi to inspect and commit the requested change, preserve unrelated work,
use the supplied release checker, and retain evidence outside the checkout.
No implementation hints or expected commit message are supplied.

## Execution

Pi 1.5.0 runs in print/JSON mode with `read,bash`, medium thinking,
extensions and MCP disabled. The configured model identifier is
`anthropic/gpt-6.1-sol`; the gateway response reports `glm-5.3-flash`.
Record both values rather than assuming the selected identifier identifies
the serving model.

The Rust checker is built with the locked dependencies from
`scripts/rustsbi-commit-cli`. Pi reads `SKILL.md`, reviews the diff and
effective identities, writes a message and exact-path request, stages
`src/lib.rs`, passes the gate, and executes the normal Git commit command.

## Observed result

- Pi exit status: `0`; checker report: `allowed: true`, two changed lines.
- Actual commit: `4428dd526a60c4c3dfe00ff1de6b297887e96562`.
- Subject: `fix: return the correct answer`.
- Author and committer: `Kozmosa <kozmosa@openatom.club>`.
- Matching DCO sign-off; subject and body satisfy the line limits.
- The commit changes only `src/lib.rs`; no tool attribution is present.
- `README.md` remains modified and `.cache/` remains untracked.
- No push or hooks installation occurs.

The external acceptance record retains the full Pi JSONL/session, request,
message, gate report, actual commit metadata/diff and final status. This
scenario verifies a real successful commit and preservation of unrelated
work. Rejected identities, attribution, oversize changes and staged-path
mismatches are covered separately by the Rust gate regression tests.
