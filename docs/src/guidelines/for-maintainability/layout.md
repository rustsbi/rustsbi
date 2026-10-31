# Layout

### Organize code for top-down reading (`top-down-reading`) {#top-down-reading}

A source file should read from top to bottom.
Start with high-level entry points and core flow.
Move implementation details downward
so readers can understand the big picture first
before diving into low-level helpers.

Within each visibility group (e.g., a module),
order methods so that callers appear before callees where possible,
enabling the file to be read top to bottom.
Place public methods before private helpers.

### Group statements into logical paragraphs (`logical-paragraphs`) {#logical-paragraphs}

Within functions,
group related statements into logical paragraphs
separated by blank lines.
Each paragraph should represent one sub-step
of the function's overall purpose.

For long functions,
add a one-line summary comment
at the start of each paragraph
when the paragraph intent is not obvious.

### Use block expressions to scope temporary state (`block-expressions`) {#block-expressions}

Use block expressions
when temporary variables are only needed
to produce one final value.
This keeps temporary state local
and avoids leaking one-off names into outer scope.

```rust
// Good — intermediate values are scoped to the block
let prober_generics = {
    let mut ans = generics.clone();
    let lifetime = Lifetime::new("'_lt", Span::mixed_site());
    ans.params
        .insert(0, GenericParam::Lifetime(LifetimeParam::new(lifetime)));
    ans
};

// Bad — temporary variables leak into outer scope
let bytes = read_bytes_from_user(addr, len as usize)?;
let socket_addr = parse_socket_addr(&bytes)?;
connect(socket_addr)?;
```

### Minimize nesting (`minimize-nesting`) {#minimize-nesting}

Minimize nesting depth.
Code nested more than three levels deep
should be reviewed for refactoring opportunities.
Each nesting level multiplies the reader's cognitive load.

Techniques for flattening nesting:
- Early returns and guard clauses for error paths.
- `let...else` to collapse `if let` chains.
- The `?` operator for error propagation.
- `continue` to skip loop iterations.
- Extracting the nested body into a helper function.

The normal/expected code path
should be the first visible path;
error and edge cases
should be handled and dismissed early.

```rust
pub(crate) fn hart_privilege_checked(hart_id: usize) -> bool {
    let Ok(hart) = HartId::from_raw(hart_id) else {
        return false;
    };
    PLATFORM
        .get()
        .is_some_and(|platform| platform.privilege_checked[hart.index()].load(Ordering::Acquire))
}
```

### Introduce explaining variables (`explain-variables`) {#explain-variables}

Break down complex expressions
by assigning intermediate results to well-named variables.
An explaining variable turns an opaque expression
into self-documenting code:

```rust
// Good — each step of the bit manipulation has a name
pub fn alloc(&mut self) -> Result<u32, PmpError> {
    loop {
        let cur_slots = self.pmp_slots.load(Ordering::Acquire);
        // check avaliable slot
        let aval_slots = !cur_slots & self.alloc_mask;
        if aval_slots == 0 {
            return Err(PmpError::NoFreeSlot);
        }

        // alloc bit
        let pmp_idx = aval_slots.trailing_zeros();
        let new_bitmap = cur_slots | (1u64 << pmp_idx);
        // set new alloc bit
        match self.pmp_slots.compare_exchange(
            cur_slots,
            new_bitmap,
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Ok(pmp_idx),
            Err(_) => continue,
        }
    }
}

// Bad — reader must parse the whole expression
debug_assert!(addr % PAGE_SIZE == 0 && addr < max_addr);
```
