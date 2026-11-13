<!-- SPDX-License-Identifier: MulanPSL-2.0 -->

# Commit checker design

The skill owns scope/message judgment and normal Git staging/commit commands.
The standalone Rust CLI checks one prospective ordinary commit from the current
index. It runs on Windows and Linux with Git and a Rust toolchain, uses structured
process arguments, and does not evaluate shell input. It has no nested model,
package installer, Git hook installer or automatic commit operation.

The request has required `repo`, `message_file`, `expected_paths`, optional
`large_change_reason`, and optional `intentional_artifacts` (exact path/reason
map). Unknown fields are refused. Output must be a new file with an existing
parent directory. Requests and messages are read as UTF-8; Git path lists are
NUL-delimited. Non-UTF-8 repository paths fail instead of being silently renamed.
Alternate indexes and merge/rebase/cherry-pick/revert workflows are out of scope.

The checker reads effective Git identities, the index, HEAD and staged numstat,
then applies message, DCO, scope, size, artifact, whitespace and module changelog
checks. Rename detection is disabled so a move contributes deletion and addition
LOC and both paths must be expected. The threshold is strictly greater than
10,000 additions plus deletions. Binary changes count as unknown LOC, appear in
the report, and require manual review; mode/gitlink changes also need inspection.
The artifact list is a heuristic, not a proof that all remaining paths are clean.

Reports include verdict/errors, paths and LOC, effective identities, HEAD,
index/message SHA-256 and the recorded exception reasons. Index state is checked
again before reporting. This detects some concurrent changes, but does not lock
out other tools, authorize exceptions or bind a later Git command. Rerun near
commit time and verify the resulting commit; hooks can change its content.
The only intended filesystem write is the explicitly selected JSON report.
Build products stay in the ignored CLI `target/`. No usage prompt, credential,
full diff or message body is stored in the report.

The canonical instructions live in `.agents/skills/rustsbi-commit`; Claude's
project entry is a relative symlink to that directory. Windows checkouts with
`core.symlinks=false` retain a target-path file instead of a usable directory;
materialize the link only on a host with native symlink support.

Natural-language correctness, English beyond obvious CJK prose, semantic focus,
proper nouns, truthful tests and real human attribution require review. A policy
pass is not a build/test result or permission to make a commit. Real human
coauthors are permitted; tool attribution checks catch known tool/bot names and
generation tags. The skill forbids adding any other tool attribution as well.

Run package validation with:

```sh
cargo fmt --manifest-path scripts/rustsbi-commit-cli/Cargo.toml -- --check
cargo clippy --locked --manifest-path scripts/rustsbi-commit-cli/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path scripts/rustsbi-commit-cli/Cargo.toml
```

Integration tests initialize temporary Git repositories and exercise actual Git
identities, staged content and trailer parsing. No project commit is made by the
checker or its validation workflow.

See [pi-acceptance.md](pi-acceptance.md) for the independent Pi commit acceptance record.
