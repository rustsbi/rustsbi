# For Development

*Is it correct and efficient?*

This is the index of coding guidelines from the **kernel developer**'s perspective.
Each guideline is anchored by a stable `short-name`
and listed with a one-line summary,
so a reader (or a review tool) can grasp the gist before opening the full text.

This persona owns everything that makes the firmware correct against its contracts:
memory safety (the discipline around `unsafe` code)
and consistency with the hardware contract (assembly conventions, CSR and SBI specifics).

## Index

**[Memory Safety](memory-safety.md)**

- [`justify-unsafe-use`](memory-safety.md#justify-unsafe-use): Justify every use of `unsafe`
- [`document-safety-conds`](memory-safety.md#document-safety-conds): Document safety conditions
- [`module-boundary-safety`](memory-safety.md#module-boundary-safety): Reason about safety at the module boundary

**[Assembly Conventions](assembly-conventions.md)**

- [`asm-function-attributes`](assembly-conventions.md#asm-function-attributes): Place attributes directly before the function
- [`asm-prefer-balign`](assembly-conventions.md#asm-prefer-balign): Prefer `.balign` over `.align`
- [`trap-entry-abi-alignment`](assembly-conventions.md#trap-entry-abi-alignment): Keep the trap entry ABI aligned
- [`save-restore-csr-around-window`](assembly-conventions.md#save-restore-csr-around-window): Save and restore CSRs around an access window

**[CSR and Register Access](csr-access.md)**

- [`gate-arch-specific-accessors`](csr-access.md#gate-arch-specific-accessors): Gate architecture-specific accessors
- [`mask-writable-bits-only`](csr-access.md#mask-writable-bits-only): Mask exactly the spec-declared writable bits
- [`no-dead-hardware-setters`](csr-access.md#no-dead-hardware-setters): Do not generate setters hardware ignores
- [`expose-effects-as-operations`](csr-access.md#expose-effects-as-operations): Expose side effects as operations
- [`single-instruction-handshake`](csr-access.md#single-instruction-handshake): Complete device handshakes in a single instruction

**[Startup and Handoff](startup-handoff.md)**

- [`publish-before-release`](startup-handoff.md#publish-before-release): Publish shared state before releasing harts
