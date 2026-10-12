# Commits

### Separate refactoring from features (`refactor-then-feature`) {#refactor-then-feature}

If a feature requires preparatory refactoring,
put the refactoring in its own commit(s)
before the feature commit.
This makes each commit easier to review and bisect.

See also:
PR [#2877](https://github.com/asterinas/asterinas/pull/2877).
