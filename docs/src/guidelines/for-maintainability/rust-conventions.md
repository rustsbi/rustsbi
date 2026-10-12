# Rust Conventions

### Create crates with kebab-case names (`kebab-case-crate-names`) {#kebab-case-crate-names}

Asterinas prefers kebab-case for crate names, which is unspecified in official Rust API Guidelines.

```rust
// Good
[package]
name = "short-vis-path"

// Bad
[package]
name = "short_vis_path"
```

### Follow Rust CamelCase and acronym capitalization (`camel-case-acronyms`) {#camel-case-acronyms}

Type names follow Rust's CamelCase convention.
Acronyms are title-cased per the Rust API Guidelines:

```rust
// Good
IoMemoryArea
PciDeviceLocation
Nvme
Tcp

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
let task_fn = self.func.take().unwrap();
let thread_fn = move || {
    let _ = oops::catch_panics_as_oops(task_fn);
    current_thread!().exit();
};

let expired_fn = move |_guard: TimerGuard| {
    ticks.fetch_add(1, Ordering::Relaxed);
    pollee.notify(IoEvents::IN);
};
```

See also:
PR [#395](https://github.com/asterinas/asterinas/pull/395#discussion_r1402964415)
and [#783](https://github.com/asterinas/asterinas/pull/783#discussion_r1593335375).

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

See also:
[Rust Reference: Diagnostic attributes](https://doc.rust-lang.org/reference/attributes/diagnostics.html)
and rustc [`unfulfilled_lint_expectations`](https://doc.rust-lang.org/rustc/lints/listing/warn-by-default.html#unfulfilled-lint-expectations).

### Sort attributes and derive traits alphabetically (`alphabetical-attrs`) {#alphabetical-attrs}

When an item carries multiple outer attributes,
list non-derive attributes in **alphabetical order** by name
and place `#[derive(...)]` **last**.
Within `#[derive(...)]`,
list the traits **alphabetically** as well.

```rust
// Good — non-derive attributes sorted; derive is last with sorted traits
#[cfg(feature = "alloc")]
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Pod)]
pub struct Foo { ... }

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

See also:
PR [#3080](https://github.com/asterinas/asterinas/pull/3080#discussion_r3031834321)
(motivating discussion)
and PR [#2898](https://github.com/asterinas/asterinas/pull/2898#discussion_r2763969731)
(earlier ad-hoc ordering choice).

### Suppress lints at the narrowest scope (`narrow-lint-suppression`) {#narrow-lint-suppression}

When suppressing lints,
the suppression should affect as little scope as possible.
This makes readers aware
of the exact places where the lint is generated
and makes it easier for subsequent committers
to maintain the suppression.

```rust
// Good — each method is individually marked
trait SomeTrait {
    #[expect(dead_code)]
    fn foo();

    #[expect(dead_code)]
    fn bar();

    fn baz();
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

See also:
[Clippy `allow_attributes`](https://rust-lang.github.io/rust-clippy/master/#allow_attributes),
[Clippy `allow_attributes_without_reason`](https://rust-lang.github.io/rust-clippy/master/#allow_attributes_without_reason),
and rustc [`unfulfilled_lint_expectations`](https://doc.rust-lang.org/rustc/lints/listing/warn-by-default.html#unfulfilled-lint-expectations).

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
/// Returns the mapping's start address.
pub fn map_to_addr(&self) -> Vaddr {
    self.map_to_addr
}

/// A policy for how [`FsPath::from_fd_at`] treats an empty `path_str`.
pub enum EmptyPathStr { /* ... */ }

/// A guard that releases a [`SpinLock`] when dropped.
pub struct SpinLockGuard<'a, T> { /* ... */ }
```

### Do not disclose implementation details in doc comments (`no-impl-in-docs`) {#no-impl-in-docs}

Doc comments should describe _what_ the API does
and _how to use it_,
not _how it is implemented internally_.

```rust
// Good — behavior-oriented
/// Returns the number of active connections.

// Bad — leaks implementation details
/// Returns the length of the internal `HashMap`
/// that tracks connections by socket address.
```
