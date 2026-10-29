# For Development

*Is it correct and efficient?*

This is the index of coding guidelines from the **kernel developer**'s perspective.
Each guideline is anchored by a stable `short-name`
and listed with a one-line summary,
so a reader (or a review tool) can grasp the gist before opening the full text.

This persona owns everything that makes the firmware correct against its contracts:
memory safety (the discipline around `unsafe` code)
and consistency with the hardware contract (assembly conventions, CSR and SBI specifics).
Adapted and new entries are being written and will follow.

## Index

**[Memory Safety](memory-safety.md)**

- [`justify-unsafe-use`](memory-safety.md#justify-unsafe-use): Justify every use of `unsafe`
- [`document-safety-conds`](memory-safety.md#document-safety-conds): Document safety conditions
- [`module-boundary-safety`](memory-safety.md#module-boundary-safety): Reason about safety at the module boundary

**[Assembly Conventions](assembly-conventions.md)**

- [`asm-function-attributes`](assembly-conventions.md#asm-function-attributes): Place attributes directly before the function
- [`asm-prefer-balign`](assembly-conventions.md#asm-prefer-balign): Prefer `.balign` over `.align`
