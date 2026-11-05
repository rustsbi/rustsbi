# Crate Documentation

### Make a crate's `README.md` its crate-level documentation (`readme-as-crate-doc`) {#readme-as-crate-doc}

A published crate's `README.md` (shown on crates.io)
and its crate-level Rust doc (shown on docs.rs)
usually carry the same content.
Keep a single source of truth:
write the `README.md`,
and include it as the crate-level doc rather than maintaining a separate copy.

```rust
#![doc = include_str!("../README.md")]
#![no_std]
#![deny(missing_docs, unstable_features)]
#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]
```

Write the `README.md` so it renders correctly under both a Markdown renderer and rustdoc.
