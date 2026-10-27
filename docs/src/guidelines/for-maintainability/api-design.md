# API Design and Visibility

### Use types to enforce invariants (`rust-type-invariants`) {#rust-type-invariants}

Leverage the type system
to make illegal states _unrepresentable_.

Define newtypes to encode domain constraints.

```rust
// Good — an `Iid` value is guaranteed to be valid
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct Iid {
    number: NonZeroU16,
}

// Bad — `u16` admits invalid values for interrupt identities
pub type Iid = u16;
```

Prefer enums over bare integers and boolean flags.

```rust
// Good — a TVM's state is constrained by the enum
/// Possible state of a TEE Virtual Machine (TVM).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum TvmState {
    /// TVM has been created, but isn’t yet ready to run.
    Initializing = 0,
    /// TVM is in a runnable state, and can be executed.
    Runnable = 1,
}

// Bad — `u32` admits invalid values
pub type TvmState = u32;
```

Encode invariants in generic parameters where needed.

```rust
/// Generic parameter `P` represents a hint of how this physical slice would be used.
/// For example, `Physical<&[u8]>` represents an immutable reference to physical byte slice,
/// while `Physical<&mut [u8]>` represents a mutable one.
///
/// An SBI implementation should load or store memory using both `phys_addr_lo` and
/// `phys_addr_hi` combined as base address. A supervisor program (kernels etc.)
/// should provide continuous physical memory, wrapping its reference using this structure
/// before passing into SBI runtime.
#[derive(Clone, Copy)]
pub struct Physical<P> {
    num_bytes: usize,
    phys_addr_lo: usize,
    phys_addr_hi: usize,
    _marker: PhantomData<P>,
}
```

Asterinas uses this pattern widely,
for example with newtypes such as `CpuId`
and `AlignedUsize<const N: u16>`.

See also:
PR [#2265](https://github.com/asterinas/asterinas/pull/2265#discussion_r2266214191)
and [#2514](https://github.com/asterinas/asterinas/pull/2514).

### Prefer enum over trait objects for closed sets (`enum-over-dyn`) {#enum-over-dyn}

When the set of variants is known and closed,
an enum is often preferable to `Box<dyn Trait>`
for both performance and pattern-matching expressiveness.

```rust
// Good — closed set modeled as an enum
#[derive(Clone, Debug)]
pub enum Case<'a> {
    /// Can't proceed test for Hart state monitor extension does not exist.
    NotExist,
    /// Test begin.
    Begin,
    /// Test failed for hart started before test begin.
    ///
    /// The returned value includes which hart led to this test failure.
    HartStartedBeforeTest(usize),
    /// Test failed for no other harts are available to be tested.
    NoStoppedHart,
    /// Test process for begin test hart state monitor on one batch.
    BatchBegin(&'a [usize]),
    /// Test process for target hart to be tested has started.
    HartStarted(usize),
    // ...
    /// Test failed for can't start target hart with [`SbiRet`] error.
    HartStartFailed {
        /// The target hart ID that has failed to start.
        hartid: usize,
        /// The `SbiRet` value for the failed hart start SBI call.
        ret: SbiRet,
    },
}
```

### Add module-level documentation for major components (`module-docs`) {#module-docs}

A module file that serves as
an important kernel component
(e.g., subsystem entry point, major data structure, driver)
should begin with a `//!` comment explaining:
1. What the module does
2. The key types it exposes
3. How it relates to neighboring modules

```rust
//! Definitions shared by all RPMI messages.
//!
//! [`MessageHeader`](crate::message::MessageHeader) and
//! [`EventHeader`](crate::message::EventHeader) store the specification's logical
//! 32-bit words. They do not choose a byte order: that belongs to the RPMI
//! transport. In particular, the shared-memory transport serializes its words
//! in little-endian order.
```

### Default to the narrowest visibility (`narrow-visibility`) {#narrow-visibility}

Start private,
then widen to `pub(super)`, `pub(crate)`, or `pub`
only when an actual external consumer requires it.

```rust
// Good — restricted to the parent module
pub(super) struct RegionAllocator<const MAX_LIVE_ALLOCATIONS: usize> {
    region: Range<usize>,
    allocations: [Range<usize>; MAX_LIVE_ALLOCATIONS],
    allocation_count: usize,
}

impl<const MAX_LIVE_ALLOCATIONS: usize> RegionAllocator<MAX_LIVE_ALLOCATIONS> {
    pub(super) fn new() -> Self {
        Self {
            region: 0..0,
            allocations: [const { 0..0 }; MAX_LIVE_ALLOCATIONS],
            allocation_count: 0,
        }
    }

    pub(super) fn initialize(&mut self, start_addr: usize, end_addr: usize) -> Result<(), ()> {
        if start_addr == 0 || start_addr >= end_addr || self.allocation_count != 0 {
            return Err(());
        }
        self.region = start_addr..end_addr;
        Ok(())
    }
}

// Bad — unnecessarily wide
pub static I8042_CONTROLLER: ...
```

The `asterinas` assembler crate does not expose any `pub` Rust items.

### Visibility modifiers encode intent, not just effect (`encode-intent-in-vis`) {#encode-intent-in-vis}

A plain `pub` says "export me as far as possible":
along its defining module path,
the item's _effective_ visibility is capped
by every enclosing module up to the crate root
(and a `pub use` elsewhere can re-export it around that cap entirely).
In a large project like Asterinas,
a `pub` item may sit deep in the module hierarchy,
making its effective visibility hard to see locally
and fragile against visibility changes in its ancestors.

Therefore, a visibility modifier in Asterinas declares
the maximum _intended_ exposure of an item,
regardless of what its ancestors happen to allow:

* `pub` means "exported from this crate".
  Reserve it for items with an actual consumer in another crate.
* `pub(crate)`, `pub(super)`, `pub(in super::super)`, and `pub(in crate::a)`
  mean "this item must never escape the named scope",
  even when an enclosing private module already restricts it further.
  If the `path` part of a `pub(in path)` modifier is long,
  apply [`short-vis-path`](#short-vis-path).

One exception is struct fields and union fields:
a `pub` field of a visibility-restricted struct or union is acceptable,
because the field sits only a few lines from the type's own modifier,
so its real visibility remains locally obvious.

This guideline is partially enforced by the rustc lint
[`unreachable_pub`](https://doc.rust-lang.org/rustc/lints/listing/allowed-by-default.html#unreachable-pub),
which flags any `pub` item not actually reachable from outside its crate.

### Qualify function calls with the parent module (`qualified-fn-imports`) {#qualified-fn-imports}

When importing a free function or a static/constant
from another module,
import the **parent module** and access the item
through it (`module::function()`, `module::CONSTANT`).
Do not import free functions or statics directly by name.

This convention is recommended by
[*The Rust Programming Language*](https://doc.rust-lang.org/book/ch07-04-bringing-paths-into-scope-with-the-use-keyword.html)
and followed by the Rust compiler codebase.
It serves two purposes:

1. The call site makes it clear
   that an imported item is being used,
   not a local one.
2. The module name provides context
   that complements the item name.

```rust
// Good — module-qualified function call
use crate::fail;

use riscv::register::mstatus;

/// Derives the next-stage address and privilege mode from the `a2`
/// `DynamicInfo`; prints and stops on invalid input.
pub(crate) fn decode_next_stage(dynamic_info_address: usize) -> (mstatus::MPP, usize) {
    let dynamic_info =
        read_dynamic_info(dynamic_info_address).unwrap_or_else(fail::no_dynamic_info_available);
    validate_next_stage(&dynamic_info).unwrap_or_else(fail::invalid_dynamic_data)
}

// Good — module-qualified static access
use ostd::mm::kspace;

let base = kspace::LINEAR_MAPPING_BASE_VADDR;

// Bad — bare function name; unclear origin at call site
use ostd::irq::disable_local;

let guard = disable_local();

// Bad — bare static name; could be mistaken for a local constant
use ostd::mm::kspace::LINEAR_MAPPING_BASE_VADDR;

let base = LINEAR_MAPPING_BASE_VADDR;
```

This guideline applies to **free functions and statics/constants**.
Types, traits, and enum variants
should still be imported directly by name,
following the standard Rust convention.

### Use workspace dependencies (`workspace-deps`) {#workspace-deps}

Always declare shared dependencies
in the workspace `[workspace.dependencies]` table
and reference them with `.workspace = true`
in member crates.

```toml
# In the workspace root Cargo.toml
[workspace.dependencies]
fdt = "0.1.5"
riscv = { version = "0.16.1", default-features = false }

# In a member crate's Cargo.toml
[dependencies]
fdt = { workspace = true }
riscv = { workspace = true }
```

### Avoid boolean arguments (`no-bool-args`) {#no-bool-args}

A boolean parameter that selects between
two behaviors signals the function does two things.
Split it into two functions
or use a typed enum.

```rust
// Good — two separate functions
fn read(&self, buf: &mut [u8]) -> Result<usize> { ... }
fn read_nonblocking(&self, buf: &mut [u8]) -> Result<usize> { ... }

// Good — typed enum
#[repr(u32)]
enum IpiState {
    Clear = 0,
    Pending = 1,
}

fn write_msip(&self, hart_id: usize, value: IpiState) -> Result<()> {
    // ...
}

// Bad — boolean argument
fn read(&self, buf: &mut [u8], blocking: bool) -> Result<usize> { ... }
```

See also:
_Clean Code_, Chapter 3 "Flag Arguments".
