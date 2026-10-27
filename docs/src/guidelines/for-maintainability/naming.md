# Naming

### Be descriptive (`descriptive-names`) {#descriptive-names}

Choose names that convey meaning at the point of use.
Avoid single-letter names and ambiguous abbreviations.
Prefer full words over cryptic shorthand
so that readers do not need surrounding context
to understand a variable's purpose.
Prefer names that are as short as possible
while still being unambiguous at the point of use.

### Be accurate (`accurate-names`) {#accurate-names}

Avoid confusing names.
If a name can be misread
to imply the wrong meaning, behavior, or side effects,
it must be corrected immediately.

```rust
// Good — clearly a count (`num_ids`) next to the collection it counts (`hart_files`)
/// Machine-level IMSIC resources selected from the Platform Description.
pub(crate) struct ImsicInfo {
    pub(crate) layout: ImsicAddressLayout,
    pub(crate) num_ids: u16,
    pub(crate) ipi_iid: Iid,
    pub(crate) hart_files: Vec<DeviceRegisterRange>,
}
// Bad — looks like a collection
// rather than a numeric counter
deleted_watches: usize
```

Choose verbs that reflect the actual work being done.

```rust
// Good — implies an indirect CSR read is involved
#[inline]
pub(super) unsafe fn read_ind(reg_id: usize) -> usize {
    interrupt::free(|| unsafe {
        let previous = miselect::read();
        miselect::write(Miselect::from_bits(reg_id));
        let value = mireg::read().bits();
        miselect::write(previous);
        value
    })
}

// Bad — looks like a plain field access
pub fn command(&self) -> Command { /* .. */ }
```

```rust
// Good — implies an O(n) collection pass over the device tree
/// Reads hart topology and features, retaining CPU interrupt wiring for IMSIC discovery.
pub(super) fn discover(
    board: &mut BoardInfo,
    platform: &PlatformView<'_>,
) -> Result<Vec<CpuInterruptController>> {
    // ...
}

// Bad — sounds like an accessor returning a reference
pub fn get_all() -> Vec<Arc<dyn Device>> { /* .. */ }
```

See also:
PR [#1488](https://github.com/asterinas/asterinas/pull/1488#discussion_r1825441287)
and [#2964](https://github.com/asterinas/asterinas/pull/2964#discussion_r2789739882).

### Use assertion-style boolean names (`bool-names`) {#bool-names}

Boolean variables and functions
should read as assertions of fact.
Use `is_`, `has_`, `can_`, `should_`, `was_`,
or `needs_` prefixes.
Never use negated names
(`is_not_empty`, `no_error`);
prefer the positive form
(`is_empty`, `ok` or `succeeded`).
A bare name like `found`, `done`, or `ready`
is acceptable when the context is unambiguous.

```rust
// Good — reads as an assertion
#[inline]
pub fn is_mem_overlap(&self, addr: usize, len: usize) -> bool {
    let self_end = self.addr + self.len;
    if let Some(target_end) = addr.checked_add(len) {
        return addr < self_end && self.addr < target_end;
    }
    true
}

// Bad — verb suggests an action, not a query
fn check_permission(&self, perm: Permission) -> bool { ... }
// Bad — negated name
let is_not_empty = !buf.is_empty();
```

See also:
PR [#1488](https://github.com/asterinas/asterinas/pull/1488#discussion_r1841827039).
