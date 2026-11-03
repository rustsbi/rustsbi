# Typed RISC-V CSR

This crate represents RISC-V control and status registers (CSRs)
with Rust types and operations.
Standard definitions follow the applicable RISC-V specifications.
Vendor-specific definitions are grouped under `vendor`
and follow the relevant vendor documentation.

## Register values

For a register with typed fields, such as `machine::Status`,
`Status<Read>` represents a value interpreted as a read result,
and `Status<Write>` represents a value prepared for a write.
The markers describe how the local value may be used;
they do not grant access to hardware.
A read value can also be constructed from raw bits,
so its type alone does not prove that it was read from a CSR.

```rust
use csr::{Write, machine::{Interrupt, MIE, Status}};

let mut value = Status::<Write>::default();
value.set::<MIE>(Interrupt::Enabled);
assert_eq!(value.get::<MIE>(), Interrupt::Enabled);
```

Field `get` and `set` operations only access the local value.
`Default` starts a write value with all bits clear;
it does not imply that the value is a hardware reset state
or a valid configuration for every use.
Some registers allow conversion from a read value to a write value,
preserving all bits, including reserved encodings.
The caller must still satisfy the register's write contract.

Not every CSR uses these markers:
some registers use a shared value type for reads and writes,
and some use a plain register-specific value type.
The individual register documentation describes its access and value types.

## Hardware access

Register access operates on the current hart.
An unavailable or inaccessible CSR can trap.
RV32-only register definitions are also available when compiling for RV64;
their availability as Rust types does not imply that the current hart can access them.
Writes and operations with hardware side effects are `unsafe`;
their callers must satisfy the documented access and execution requirements.
The types do not establish that a hardware transition is safe.
The generated `modify` operation performs a read followed by a write
and is not atomic.

The crate represents register layouts, values, and hardware operations.
The firmware using it owns policy and coordination,
including feature locks, initialization, interrupt delivery, and recovery.

## Sources and safety

Every register, field, encoding, constraint, and hardware operation
must be traceable to the applicable RISC-V specification or vendor documentation.
Hand-written `struct`, `enum`, `impl`, and `unsafe` operations
also need a reference for their hardware meaning and behavior.
Cite the relevant section or stable anchor at the item,
or at an enclosing type or module when it clearly covers the definitions.
Use a specific section when one is available.

Check each definition against the cited ISA version, XLEN, privilege level,
optional extensions, reserved encodings, access restrictions, and side effects.
Document how the Rust API represents the source;
do not present a Rust design choice as a hardware guarantee.
The `csr!` macro can generate types and access operations from declarations.
Hand-written types and helpers belong here when they directly represent
a documented hardware value, constraint, or operation.
An instruction's existence alone does not justify an unrelated convenience API.

A hardware reference and a Rust safety argument serve different purposes.
Document caller obligations for each unsafe function in a `# Safety` section,
and justify each unsafe block at its use site.
Neither a specification citation nor a safety comment alone establishes both
architectural correctness and Rust soundness.

Vendor-specific definitions must identify the vendor and applicable hardware
and remain distinct from standard definitions.
Implementation sources can support a reference,
but do not replace the governing specification or vendor documentation.
When a definition's source or ownership is unclear,
identify the gap before claiming it conforms to this design.
