<!-- SPDX-License-Identifier: MulanPSL-2.0 -->

# Commit rules and sources

The source order is the user's explicit instructions, the selected checkout's
contribution rules and CI, then local history for naming and useful detail.
Historical commits can violate current rules; do not copy their mistakes.

## Repository conventions

The RustSBI contribution guide at
[`slides/2025/reports/Contributing to RustSBI.md`](https://github.com/rustsbi/slides/blob/main/2025/reports/Contributing%20to%20RustSBI.md)
requires English, one problem per commit, a header and footer, and a body for
direct contributions. Organize the explanation around what, why and how;
these do not need literal headings. Use imperative language and describe the
actual change, including material validation limits.

The header accepts `<scope or type>: <subject>` or
`<type>(<scope>): <subject>`, with optional `!` for a breaking change.
Documented types are `feat`, `fix`, `build`, `refactor`, `docs`, `ci`, `test`.
Module-only prefixes such as `rt`, `spec` and `deps` are also valid.
Use the nearest relevant historical module prefix; do not invent a global
type taxonomy or force all history into `type(scope)`.

The workspace's `imperative-subject` rule in
`.guidelines/for-maintainability/commits.md` applies the <=72-character limit
to the complete first line, including the prefix; prefer <=50.
Use this stricter interpretation when preparing a commit.
The separate 100-character limit applies to every full line, including the
prefix and DCO trailers. A short subject can still produce an overlong header.
Do not wrap an identity trailer into invalid syntax to fit the width.

The selected checkout's `.github/workflows/DCO.yml` is authoritative for its
CI check. At the implementation baseline `7fef2fe`, it parses final trailers
with `git interpret-trailers --parse` and accepts a sign-off matching author
**or** committer (case-insensitive). This checker uses the same criterion for
an ordinary prospective commit and reads effective identities through Git.
It does not infer legal DCO certification from the presence of a string.

`.github/workflows/Changelog.yml` requires a changed, retained CHANGELOG when
the diff touches `library/{sbi-rt,sbi-spec,sbi-testing,rustsbi,macros}/`.
Review the content as well: stage the module's relevant entry, not a fabricated
or unrelated changelog edit. CI/skill-only changes do not need a library entry.
Reconcile the checker if those workflow rules change.

Baseline history includes `7fef2fe` (`fix(skill): ...`), `ebc7190`
(`refactor(prototyper): ...`) and `2690688` (`ci: ...`): precise headers,
reasoned bodies, and human sign-offs. Some older one-line fixes omit a body;
that is not an exception to the direct-contribution rule.

## Additional developer policy

The developer explicitly requires non-noreply submission emails, no tool or
bot authorship/coauthor credits, no unexpected files, and a default block for
diffs over 10,000 changed text lines. These are stricter than the DCO workflow.
Both effective author and committer emails must satisfy the email policy.

Only the developer can supply an exception for an intended large change or
intended artifact. Record the concrete reason in the request/report. There is
no override for DCO, message width, noreply addresses or tool attribution.

```text
fix(prototyper): preserve the boot hart during discovery

Keep the selected boot hart when rebuilding the platform hart list.
This prevents discovery from selecting an unavailable hart on restart.

Add a regression case covering the reordered discovery result.

Signed-off-by: <the developer's configured name and non-noreply email>
```

Replace the illustrative footer with the developer's actual identity; never
reuse names or emails from examples or history.
