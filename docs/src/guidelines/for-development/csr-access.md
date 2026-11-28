# CSR and Register Access

`riscv-csr-spec` owns portable CSR numbers, field encodings, and value types.
Standard definitions live under `standard::isa` and `standard::aia`.
Vendor-specific definitions live under `vendor`. Shared ISA values belong to a named domain
such as `values::trap` or `values::xlen`. Values used by one register family
stay beside that family's definitions.

The internal `rustsbi-csr` crate re-exports these definitions and provides
`Read`, `Write`, and `Claim` hardware access. `Read` and `Write` provide `try_*`
methods that execute recoverable accesses and emit fault records. Runtime owns the trap handler,
record-table lookup, capability detection, interrupt exclusion, and initialization.
Firmware policy uses Runtime and the portable definitions.

### Separate capabilities, instruction effects and Rust safety {#csr-access-contracts}

Spec declarations use `r`, `w` and `claim` for permitted operations. Keep Rust
`unsafe` classification on execution methods: standard `Read::read` is safe and
requires `StandardRead`. Vendor reads use `unsafe Read::read_unchecked`
with hardware applicability and read-effect requirements. `Write::write`,
`Claim::claim` and recoverable `try_*` accesses are unsafe.
Record register-specific hardware preconditions under `Access requirements`.
The executor's unsafe methods document the caller's obligations under `Safety`.
State what a successful access changes, which result the caller may rely on,
and what can already have changed if a later instruction traps. Cite the
specification section that defines the behavior once per register family.
Standard `r` declarations implement `StandardRead` automatically.

Recovery preserves general registers except declared outputs and `t2`/`t3`, but
changes machine trap CSRs and trap-return status. Preserve any live return context
explicitly, or complete probes before preparing it. A handler that uses
floating-point or vector registers must preserve their live state.

`csrr` (`csrrs rd, csr, x0`) reads the CSR and suppresses the write. Zicsr
specifies that standard CSRs have no read side effects. Custom CSRs may define
them. `csrw` (`csrrw x0, csr, rs1`) suppresses
the read. With a nonzero destination, `csrrw rd, csr, x0` reads the old value
and writes zero. `csrrs` and `csrrc` always read. Their register forms suppress
the write when `rs1` names `x0`. Their immediate forms suppress it for a zero mask.
See [Zicsr](https://docs.riscv.org/reference/isa/v20260120/unpriv/zicsr.html).

`ReadMode` and `WriteMode` describe local snapshots and prepared values. They do
not execute instructions or classify Rust safety. Type bounds prevent forbidden
operand combinations, including inside unsafe blocks; raw values still require
hardware-specific validation and caller-established state.

### Gate architecture-specific accessors (`gate-arch-specific-accessors`) {#gate-arch-specific-accessors}

A CSR that exists only at one XLEN (e.g. the RV32-only high-half registers
`mieh`, `miph`, `hviprio1h`) must have its accessor compiled only for that
target. On the other XLEN the accessor must not compile down to a CSR
instruction — a CSR number that is unimplemented on the running hart raises an
illegal-instruction exception at runtime, not at compile time.

Portable definitions remain available for decoding on any host.
`Width::Rv32Only` covers both high-half CSRs such as `mstatush` and standalone
addresses such as `pmpcfg1`. The hardware access traits reject them on RV64 at compile time.
Runtime groups hardware modules behind an architecture condition at their
module entry. Individual hardware functions need no non-RISC-V placeholder.
Keep XLEN conditions where the instruction or register layout actually differs:

```rust
#[cfg(target_pointer_width = "32")]
use csr::standard::isa::privileged::machine::Mstatush;
```

A hand-written instruction must enforce the same availability constraint:

```rust
// Bad — a custom accessor without cfg gating
// emits the CSR instruction on RV64 too
#[inline]
pub fn read_mieh() -> usize {
    let bits: usize;
    // SAFETY: mieh exists only on RV32; on RV64 this traps at runtime.
    unsafe {
        core::arch::asm!("csrrs {0}, {1}, x0", out(reg) bits, const 0x314);
    }
    bits
}
```

For logical 64-bit values such as `stimecmp`, expose one value on both XLENs.
The hardware accessor selects the RV32 high/low sequence or the RV64 instruction.

### Mask exactly the spec-declared writable bits (`mask-writable-bits-only`) {#mask-writable-bits-only}

Define each writable field for the ISA or extension that specifies it. For a
conditional alias, document which control bits select its writable backing
field. For WPRI fields, software preserves the reserved bits on writes and
ignores them on reads. For WARL fields, a completed write can read back a
different legal value. Software that depends on the selected value must verify
it before using it. [CSR field specifications](https://docs.riscv.org/reference/isa/v20260120/priv/priv-csrs.html#1-1-3-csr-field-specifications)

### Do not generate setters hardware ignores (`no-dead-hardware-setters`) {#no-dead-hardware-setters}

Expose device-driven `mip.MTIP` and `mip.MEIP` for reading. Changing the timer
comparator or the interrupt controller changes the underlying pending signal.
For software-injected interrupts, expose the operations permitted by the
backing register, such as atomic set and clear on `hvip.VSSIP`.

AIA standard local pending bits, including implemented RAS interrupts 35 and 43,
are writable and remain set until software clears them. Expose restricted atomic
operations on `Mip`/`Sip`/`Vsip` for RV64 and `Miph`/`Siph`/`Vsiph` for RV32.
`Sip::SEIP` also permits writes when AIA selects the writable `mvip` alias
(`mideleg[9] = 0`, `mvien[9] = 1`). Document these runtime conditions instead
of treating the field as unconditionally read-only.
See [AIA v1.0, machine and supervisor interrupts](https://docs.riscv.org/reference/aia/v1.0/MSLevel.html).

### Expose side effects as operations (`expose-effects-as-operations`) {#expose-effects-as-operations}

Writing an IMSIC `*topei` claims the reported interrupt if one is pending.
The written value is ignored. `Claim::claim` returns the decoded value from a
single `csrrw` and requires the caller to own claims on that interrupt file.
A plain `csrr` can inspect the same report without acknowledging it.

See [Claim device interrupts atomically](sbi-specific.md#atomic-csr-claim)
for the AIA claim instruction.

### Complete device handshakes in a single instruction (`single-instruction-handshake`) {#single-instruction-handshake}

A higher-priority interrupt can arrive between a read of `*topei` and a separate
claim. Use the value returned by the claim itself to identify the interrupt to service.
