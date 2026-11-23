# Crate Documentation

### Make a crate's `README.md` its crate-level documentation (`readme-as-crate-doc`) {#readme-as-crate-doc}

A published crate's `README.md` (shown on crates.io)
and its crate-level Rust doc (shown on docs.rs)
usually carry the same content.
Keep a single source of truth:
write the `README.md`,
and include it as the crate-level doc rather than maintaining a separate copy.
The same README serves as the repository introduction,
the crate's crates.io page,
and its docs.rs homepage,
so only one copy of the content needs to be maintained.

```rust
#![doc = include_str!("../README.md")]
```

Write the `README.md` so it renders correctly under both a Markdown renderer and rustdoc.
