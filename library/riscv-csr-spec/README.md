# `riscv-csr-spec`

Portable definitions of RISC-V control and status registers (CSRs).
Use this `no_std` crate to decode register snapshots and prepare values for hardware access.
The separate `rustsbi-csr` crate executes the corresponding instructions.

`standard::isa` groups registers by privilege level and provides their shared field encodings.
`standard::aia` provides the Advanced Interrupt Architecture registers and IMSIC values.
`vendor` groups custom registers by implementation.
For example, T-Head and SpacemiT assign different meanings to CSR number `0x7c0`.

## Working with a register value

```rust
use riscv_csr_spec::standard::isa::{
    privileged::machine::Mstatus,
    values::status::Privilege,
};

let observed = Mstatus::from_bits(1 << 11);
assert_eq!(observed.get(Mstatus::MPP), Ok(Privilege::Supervisor));

let mut next = observed.into_write();
next.set(Mstatus::MIE, true);
assert_eq!(next.bits(), (1 << 11) | (1 << 3));
```

`from_bits` and `bits` preserve the raw encoding.
For fields with reserved encodings, `get` returns the original field bits in `Err`.
`into_write` preserves every bit of the snapshot, including reserved bits.
`set` changes only the selected field and panics if the supplied encoding exceeds its width.
`zero` starts with all bits clear. The reset value and legal settings depend on the register.

## Hardware access

`ReadSpec`, `WriteSpec<V>`, and `ClaimSpec` describe the operations available for each CSR.
The executor can read standard registers implementing `StandardRead` through its safe `read` method.
Reads of custom registers follow the vendor's access and side-effect requirements.
A write accepts the operand types declared for that register, such as `Set` or `Clear` for selected bits.
A claim combines a read with a write of zero in one `csrrw` instruction.
The register documentation describes the required privilege, extensions, and synchronization.

Hardware can ignore writes to read-only fields or select a different legal value for a WARL field.
Software must preserve WPRI bits where required.
Even a masked `CSRRS` or `CSRRC` can affect an unselected bit if its value on read differs from the underlying state.
See the [CSR field conventions](https://docs.riscv.org/reference/isa/v20260120/priv/priv-csrs.html#1-1-3-csr-field-specifications)
and [Zicsr instruction semantics](https://docs.riscv.org/reference/isa/v20260120/unpriv/zicsr.html).

## Register widths

Paired registers, such as `Menvcfg` and `Mcycle`, use a 64-bit value on both RV32 and RV64.
On RV32, `NUMBER` and `HIGH_NUMBER` identify the low and high words.
`Width::Rv32Only` covers both high-half registers such as `Mstatush` and odd-numbered PMP configuration registers such as `Pmpcfg<1>`.
When decoding a saved trap cause, pass the effective XLEN of the source CSR so the decoder locates the interrupt bit correctly.
