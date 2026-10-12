# Coding Guidelines

This chapter collects the coding and collaboration conventions for the RustSBI project.
The guidelines set the same standard for both **writing** code and **reviewing** it.
Each guideline is anchored by a stable `short-name`
so that reviewers and review tools can cite it.

The guidelines are organized by **persona**:
five durable engineering roles,
each with its own checklist.
A guideline belongs to the persona that is the natural owner of the failure it prevents.

| Persona | Focus | Guidelines |
|---|---|---|
| Project maintainer | Is the code well-shaped and understandable? | [For Maintainability](for-maintainability/README.md) |
| Kernel developer | Is it correct and efficient? | [For Development](for-development/README.md) |
| Security expert | Is it safe and secure? | [For Security](for-security/README.md) |
| Hardware expert | Is it correct against the hardware contract? | [For Hardware](for-hardware/README.md) |
| Documentation writer | Are the user-facing docs well-written? | [For Documentation](for-documentation/README.md) |

## Sources and Status

The guidelines come from three sources:
entries inherited verbatim from the [Asterinas coding guidelines](https://asterinas.github.io/book/to-contribute/coding-guidelines/index.html),
entries adapted from them to the RISC-V SBI firmware context,
and new entries distilled from RustSBI's own contribution rules, CI requirements, and review history.

This chapter currently carries only the **inherited** entries,
copied verbatim from the Asterinas originals.
Adapted and new entries are being written and will follow.

The Asterinas coding guidelines are dual-licensed under MPL-2.0 and Apache-2.0.
Thanks to the Asterinas project.
