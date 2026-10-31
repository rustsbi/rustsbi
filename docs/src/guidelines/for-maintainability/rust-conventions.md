# Rust Conventions

### Create crates with kebab-case names (`kebab-case-crate-names`) {#kebab-case-crate-names}

Asterinas prefers kebab-case for crate names, which is unspecified in official Rust API Guidelines.

```toml
// Good
[package]
name = "rustsbi-csr"
description = "Typed RISC-V CSR access for RustSBI firmware"
version = "0.0.0"

// Bad
[package]
name = "short_vis_path"
```

### Follow Rust CamelCase and acronym capitalization (`camel-case-acronyms`) {#camel-case-acronyms}

Type names follow Rust's CamelCase convention.
Acronyms are title-cased per the Rust API Guidelines:

```rust
// Good
SbiRet
IpiRequest
PmpConfig
HartId

// Bad
IOMemoryArea
PCIDeviceLocation
NVMe
TCP
```

### End closure variables with `_fn` (`closure-fn-suffix`) {#closure-fn-suffix}

Variables holding closures or function pointers
must signal they are callable by ending with `_fn`.
Treating a closure variable
as if it were a data object misleads readers.

```rust
// Good — clearly a callable
let read_time_fn = || {
    timer
        .read_time_low()
        .map(|value| value as u32)
        .ok_or(Error::NotEnoughResources)
};
let start = read_time_fn()?;
```

### Use `#[expect(dead_code)]` with restraint (`expect-dead-code`) {#expect-dead-code}

In general, dead code should be avoided because
_(i)_ it introduces unnecessary maintenance overhead, and
_(ii)_ its correctness can only be guaranteed
by manual and error-prone review.

Dead code is acceptable only when all of these hold:

1. A _concrete case_ will be implemented in the future
   that turns the dead code into used code.
2. The semantics are _clear_ enough,
   even without the use case.
3. The dead code is _simple_ enough
   that both the committer and the reviewer
   can be confident it is correct without testing.
4. It serves as a counterpart to existing non-dead code.

For example, it is fine to add ABI constants
that are unused because the corresponding feature
is partially implemented.

### Sort attributes and derive traits alphabetically (`alphabetical-attrs`) {#alphabetical-attrs}

When an item carries multiple outer attributes,
list non-derive attributes in **alphabetical order** by name
and place `#[derive(...)]` **last**.
Within `#[derive(...)]`,
list the traits **alphabetically** as well.

```rust
// Good — non-derive attributes sorted; derive is last with sorted traits
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PhysAddr(usize);

// Bad — arbitrary ordering
#[derive(Debug, Default, Clone, Copy, Pod)]
#[cfg(feature = "alloc")]
#[repr(C)]
pub struct Foo { ... }
```

Placing `#[derive(...)]` last ensures
that derive macros always see the item
after all attribute macros
(e.g., `#[padding_struct]`, `#[pod_union]`)
have transformed it.
Derive helper attributes
(e.g., `#[serde(...)]`, `#[clap(...)]`)
stay immediately after `#[derive(...)]`.
Sorting the remaining attributes alphabetically
eliminates hesitation over placement
and reduces noise in diffs.

### Suppress lints at the narrowest scope (`narrow-lint-suppression`) {#narrow-lint-suppression}

When suppressing lints,
the suppression should affect as little scope as possible.
This makes readers aware
of the exact places where the lint is generated
and makes it easier for subsequent committers
to maintain the suppression.

```rust
// Good — only the single function that triggers the lint is marked, with a reason
#[expect(
    clippy::result_large_err,
    reason = "fixed-size reports preserve all failures without requiring a heap allocator"
)]
pub(super) fn test() -> TestResult {
    // ...
}

// Bad — the entire trait is suppressed
#[expect(dead_code)]
trait SomeTrait { ... }
```

There is one exception:
if it is clear enough
that every member will trigger the lint,
it is reasonable to expect the lint at the type level.

```rust
#[expect(non_camel_case_types)]
enum SomeEnum {
    FOO_ABC,
    BAR_DEF,
}
```

### Follow RFC 1574 summary line conventions (`rfc1574-summary`) {#rfc1574-summary}

The first line of a doc comment should be concise and one sentence.
Its grammatical form depends on what the item is:

- **Functions and methods** — third-person singular present indicative verb
  ("Returns", "Creates", "Acquires"), describing the action performed.
- **Types (structs, enums, traits, type aliases), modules, and fields** —
  a noun phrase naming the thing, not describing an action.
  This matches the Rust standard library convention
  (e.g., `Vec` is "A contiguous growable array type").

```rust
/// Returns the underlying interrupt identity number as `u16`.
#[inline]
pub const fn number(self) -> u16 {
    self.number.get()
}

/// Iterator structure for `HartMask`.
///
/// It will iterate hart id from low to high.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct HartIds {
    inner: UnvisitedMask,
}
```

### Do not disclose implementation details in doc comments (`no-impl-in-docs`) {#no-impl-in-docs}

Doc comments should describe _what_ the API does
and _how to use it_,
not _how it is implemented internally_.

```rust
// Good — behavior-oriented
/// RISC-V AIA Interrupt Identity (IID).
///
/// An IID is the encoded identity used by AIA/IMSIC to refer to an interrupt.
/// Value `0` is reserved/invalid. Valid identities are in the range `1..=N`.

// Bad — leaks implementation details
/// Returns the length of the internal `HashMap`
/// that tracks connections by socket address.
```
