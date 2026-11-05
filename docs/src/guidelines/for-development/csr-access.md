# CSR and Register Access
### Gate architecture-specific accessors (`gate-arch-specific-accessors`) {#gate-arch-specific-accessors}

A CSR that exists only at one XLEN (e.g. the RV32-only high-half registers
`mieh`, `miph`, `hviprio1h`) must have its accessor compiled only for that
target. On the other XLEN the accessor must not compile down to a CSR
instruction — a CSR number that is unimplemented on the running hart raises an
illegal-instruction exception at runtime, not at compile time.

Gate the whole accessor, not just the field helpers:

```rust
// Good — the access path itself is cfg-gated
#[cfg(target_arch = "riscv32")]
riscv::read_csr_as_rv32!(Mieh, 0x314);
```

```rust
// Bad — as_rv32 macro emits the CSR instruction on RV64 too
riscv::read_csr_as_rv32!(Mieh, 0x314);
```

If a logical register has an RV32 half and an RV64 wide form
(like `sie`/`sieh`), expose one API and dispatch internally,
so callers cannot reach an unreachable half.

### Mask exactly the spec-declared writable bits (`mask-writable-bits-only`) {#mask-writable-bits-only}

A CSR wrapper's bit mask must equal the writable bits the specification
declares for the current XLEN — no more, no less.
A too-wide mask lets callers write reserved bits (silently WPRI-dropped,
or trapping on strict harts);
a too-narrow mask hides writable ones.
When RV32 and RV64 differ, state both values in a comment next to the mask
and derive per-XLEN fields with `cfg(target_pointer_width = ...)`.

```rust
// Good — mask comes from the spec, per XLEN, and says so
riscv::read_write_csr! {
    Sie: 0x104,
    // WPRI elsewhere; writable: 1 SSIE, 5 STIE, 9 SEIE, 13 LCOFI
    // RV32: 0x0000_2222, RV64: 0x0000_0000_0000_2222
    mask: 0x2222,
}
```

```rust
// Bad — writable bits of `sie` are only SSIE|STIE|SEIE|LCOFI = 0x2222
riscv::read_write_csr! {
    Sie: 0x104,
    mask: usize::MAX & !0x1DDD, // includes many read-only bits
}
```

The same rule applies to read/write *permissions* of individual fields:
a bit the spec makes read-only must not get a field setter,
and a bit the spec makes writable must not be wrapped in a read-only
field macro.

### Do not generate setters hardware ignores (`no-dead-hardware-setters`) {#no-dead-hardware-setters}

Software cannot set a pending bit that only a device or the hart itself
drives (e.g. most bits of `mip`/`miph`).
A setter that compiles and silently does nothing
(`csrrs` with a bit the hardware will not set)
is worse than no setter: it looks like an API and lies.
Do not emit `set_*`/`clear_*` helpers for such bits;
if a bit is genuinely software-writable (like VSSIP in `hvip`),
it is not in this category and must have one.

```rust
// Bad — pending bits of miph are device-driven; these setters no-op
riscv::set_clear_csr!(
    /// Low-priority RAS event interrupt pending.
    , set_low_priority_ras_event, clear_low_priority_ras_event, 1 << 3);
```

### Expose side effects as operations (`expose-effects-as-operations`) {#expose-effects-as-operations}

Some register writes are not "writes" — they are actions.
Writing `*topei` claims an interrupt;
writing certain config registers triggers a fence or invalidation.
For those, do not expose `write(value)` with an ignored or echoed operand.
Expose the *operation* with an explicit name,
so the call site reads as what happens,
and the value the hardware discards is not promoted to a parameter.

```rust
// Good — the claim is the API; the discarded operand is not a parameter
pub fn claim(iid: Iid) -> Option<Iid> { /* single csrrw, see below */ }
```

```rust
// Bad — looks like state assignment, hides the claim side effect
mtopei::write(Mtopei::from_bits(...));
```

### Complete device handshakes in a single instruction (`single-instruction-handshake`) {#single-instruction-handshake}

For registers shared with a device (e.g. `*topei` with the IMSIC),
a read-modify-write in two instructions is not atomic:
the device may change the register between the `csrr` and the `csrrw`,
so you claim or write back stale state.
Perform the read-modify-write as a single `csrrw`
(or `csrrs`/`csrc`) instruction,
letting the hardware do the atomic exchange.

```rust
// Good — one atomic exchange
// csrrw with the new value; the old value comes back in rd

// Bad — two instructions; the IMSIC may update mtopei in between
let v = mtopei::read();
mtopei::write(v.with_iid(0));
```

This is the instruction-level analogue of the atomicity requirement
in memory-safety: no torn handshakes with hardware.
