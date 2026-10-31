# Comments

### Explain why, not what (`explain-why`) {#explain-why}

Comments should explain the intent behind the code,
not restate what the code does.
If a comment merely paraphrases the code,
it adds noise without insight.

If a comment is needed to explain what code does,
first try to rewrite the code.
Do not write good comments to compensate for bad code —
rewrite it to be straightforward.

### Document design decisions (`design-decisions`) {#design-decisions}

When the code makes a non-obvious choice —
a particular data structure, a locking strategy,
a deviation from Linux behavior —
add a comment explaining the rationale
and any alternatives considered.
Design-decision comments ("director's commentary")
are the most valuable kind of comment.

```rust
// Compressed accesses are forwarded until XLEN-dependent encodings are supported.
// Floating-point accesses are forwarded because entry does not save those registers.
// Encoding reference: https://github.com/riscv-software-src/opensbi/blob/master/lib/sbi/sbi_trap_ldst.c
```

### End sentence comments with punctuation (`comment-punctuation`) {#comment-punctuation}

If a comment line is a full sentence,
end it with proper punctuation.
This improves readability in dense code
and avoids fragmented prose.

```rust
// Good — complete sentence with punctuation.
// SAFETY: reading this CSR has no memory-safety preconditions. A caller
// must still ensure that the current hart implements an IMSIC.

// Bad — complete sentence without punctuation
// SAFETY: The pointer is derived from a live allocation
```
