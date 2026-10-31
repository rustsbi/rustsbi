# Cases

Ground truth for the backtest, one file per pull request.

A case is the **real review feedback** that maintainers left on the PR.
It is the positive test set: run the skill on `pr/<N>` and compare its comments with these.

## Format

```markdown
---
pr: 412
repo: rustsbi/rustsbi
base: 67120f4aa65a3531eaf326276542e1148804a76d
head: 1c1b06900b276357a3326cb619ef40ec9acd1c05
gt: 2
---

### `firmware/runtime/src/hart/context.rs` line 88

> <the reviewer's comment, verbatim>
```

- `gt` — number of ground-truth comments in the file.
- Each comment is a `###` heading of the form ```path``` line `N``, followed by a quote (`>`) per comment line.
- Copy the text **verbatim**. Do not summarise, do not repair the line number, do not drop a comment
  because it looks uninteresting: any edit biases the measurement.
- Record the `revision` the comments were written against (a SHA in the PR's commit list) and review with
  `diff pr/<N>@<revision>`. Reviewing the PR head instead scores the reviewer against defects the PR
  already fixed in its own follow-up commits.

## Adjudicating a run

Do not score mechanically in this phase. Per run, hand the ground truth and the
run's comments to a fresh adjudicator (a sub-agent with the two lists inlined, no
tools) and ask for item-by-item verdicts:

- per ground-truth item: `REPRODUCED` / `PARTIAL` / `MISSED`, and which produced
  comment matched. A cross-cutting defect reported once anywhere counts as reproduced.
- per produced comment: `IN-GT` / `VALID-NEW` (a real defect the humans did not
  mention) / `WRONG` (false, or a misreading of the hardware contract).
- the total: "reproduced X of N", against the bar of **half**.

`score.py` stays for the later regression stage; its file+line tolerance is not a
verdict, and on a cross-cutting defect it punishes a correct finding for landing on
a different file than the human chose.

## Classifying the ground truth

Real review comments are of three kinds. Only the first is a fair test of the skill:

| class | meaning | counts? |
|---|---|---|
| `code-derivable` | decidable from the diff (a missing `// SAFETY:`, `.align` that should be `.balign`) | yes |
| `contextual` | needs maintainer-only context ("the interface changes next week") | no |
| `process` | workflow ("split this commit", "add CI") | no |

Record the class in a trailing tag on the quote line, e.g. `> [code-derivable] ...`,
and let `score.py` ignore the other two.
