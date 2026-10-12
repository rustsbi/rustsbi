# General Style

### Prefer semantic line breaks (`semantic-line-breaks`) {#semantic-line-breaks}

For prose in Markdown and doc comments,
insert line breaks at semantic boundaries
so each line carries one coherent idea.
At minimum, break at sentence boundaries.
For longer sentences, also consider breaking at clause boundaries.

Semantic line breaks make diffs smaller,
reviews easier,
and merge conflicts less noisy.

As an exception,
RFC documents that are mostly read-only
can use regular paragraph wrapping.

See also:
[Semantic Line Breaks](https://sembr.org/).
