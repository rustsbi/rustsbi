---
# SPDX-License-Identifier: MulanPSL-2.0
name: rustsbi-commit
description: Prepare and create focused RustSBI Git commits following repository message, DCO, identity and staged-change conventions. Use when asked to commit changes, draft or check a commit message, or diagnose a commit hygiene failure.
license: MulanPSL-2.0
---

# RustSBI commits

Use the selected checkout's contribution rules and recent commits for module
prefixes and wording. Read [references/conventions.md](references/conventions.md)
for the repository rules and the additional local commit policy.
The Rust checker enforces mechanical constraints; review the actual diff and
message for intent before creating a commit.

For a draft/message-only request, review the message and report the result
without staging or committing. The CLI below checks a complete prospective
commit; use it when the intended index is available, and do not manufacture
staged changes just to check a draft.

## Prepare the exact change

1. Inspect branch, HEAD, `git status --short`, staged and unstaged diffs,
   untracked paths, and recent commit messages. Preserve existing staging.
   Identify the requested changes before deciding which paths to stage.
   A request to create this skill or check a message does not authorize a commit.
2. Read every candidate diff, including mode changes, deletions and binary files.
   Stage explicit paths with `git add -- <paths>` or selected hunks when a file
   contains unrelated work. Never use blanket `git add .`, `git add -A` or
   `git commit -a`. Do not erase or stash unrelated edits to simplify the commit.
   Split changes that solve independent problems into separate commits.
3. Resolve effective identities with `git var GIT_AUTHOR_IDENT` and
   `git var GIT_COMMITTER_IDENT`; environment overrides can differ from config.
   Require the developer's real configured name and a reachable non-noreply
   email. If missing or wrong, obtain the developer's identity; never invent an
   address, change global Git configuration or reuse another contributor's.
4. Draft an English message in a UTF-8 file outside the checkout.
   Use `type(scope): subject` or the repository's `scope/type: subject` form.
   Subject: imperative, usually lowercase except proper names/code, no final
   period; the complete header including its prefix is preferably <=50 and at
   most 72 characters. Every complete line,
   including header, body and trailers, must fit in 100 characters.
   Explain what changed and why, adding how and relevant validation when useful.
   Include a final `Signed-off-by: Name <email>` matching the effective author
   or committer. The sign-off records the developer's DCO certification;
   do not impersonate another person or treat a tool as a signer.
   Do not add Claude, Codex or other tool/bot coauthors, generation credits or
   assisted-by tags. Preserve legitimate human attribution when applicable.
   Keep issue references precise: use `Fix`/`Fixes`/`Closes` only when this
   change resolves the issue, and `Refs` for related context. Mark breaking
   changes with `!` or `BREAKING CHANGE` and synchronize the relevant docs.

## Check, then commit when requested

All deterministic implementation lives in `scripts/rustsbi-commit-cli` (Rust).
Build it with an installed host toolchain; no installer is needed:

```sh
RUSTUP_AUTO_INSTALL=0 cargo build --release --locked --manifest-path /absolute/skill/scripts/rustsbi-commit-cli/Cargo.toml
```

Use the resulting `scripts/rustsbi-commit-cli/target/release/rustsbi-commit-cli` executable
(`.exe` on Windows). If the repository's pinned toolchain is unavailable, select
an already installed host toolchain explicitly rather than installing one.

Write a request JSON outside the checkout:

```json
{
  "repo": "/work/rustsbi",
  "message_file": "/tmp/rustsbi-message.txt",
  "expected_paths": ["firmware/prototyper/src/example.rs"]
}
```

The path list comes from the intended change and hunk review, not a blind copy
of everything currently staged. Paths are exact, repository-relative and use
forward slashes. For renames include both old and new paths.
Request-relative repository/message paths are supported.

```sh
/absolute/skill/scripts/rustsbi-commit-cli/target/release/rustsbi-commit-cli \
    check /tmp/rustsbi-request.json /tmp/rustsbi-check-1.json
```

Exit `0` permits the mechanical checks, `1` blocks on a rule violation, and `2`
means invalid input or an incomplete check. Read the JSON report and resolve
blockers before proceeding; existing reports are never overwritten.
The checker neither stages changes nor makes commits, edits config or installs
hooks. It checks the current index, not unstaged working-tree contents.

- Diff size counts additions plus deletions without rename detection.
  More than **10,000 lines** blocks by default. Only an explicit developer
  statement that the large change is intended permits `large_change_reason`
  in the request; quote or faithfully record that statement. A broad request
  to commit everything is not an exception by itself. Splitting an unrelated
  dump into mechanical chunks does not make it appropriate.
- Cache/temp/log paths block unless the developer explicitly intended that
  exact file (for example a regression fixture). In that case add an exact
  `intentional_artifacts` path-to-reason mapping. This never permits unrelated
  files. Inspect binaries separately; they have no text LOC count.
- The checker also enforces staged whitespace and the five module CHANGELOG
  requirements from the current CI. Run relevant build/test checks separately;
  a hygiene pass does not prove correctness.
  Respect any smaller review-size limit in the selected guidelines; the 10k
  guard is not a recommendation to pack every smaller change into one commit.

After any staging, message or identity change, rerun the checker to a new report.
Immediately before committing, review `git diff --cached` once more and ensure
no other process is changing the index. Keep request files and reports outside
the commit. When the user requested the commit, run:

```sh
git commit --file /tmp/rustsbi-message.txt --cleanup=verbatim
```

The message already includes the checked sign-off. Keep normal hooks enabled;
never use `--no-verify` or amend earlier commits without authorization.
If a hook changes files/message or fails, inspect its effect and recheck the
result instead of bypassing it. Verify the resulting commit with
`git show --format=fuller --stat HEAD`, its full message and diff, then inspect
`git status --short`. Confirm actual identities, DCO, attribution and scope;
report the hash, validation and remaining edits. If post-checks fail, report
the concrete defect and obtain authorization before rewriting history.

See [spec/README.md](spec/README.md) for checker scope and testing.
