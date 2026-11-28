# `rustsbi-csr` (`csr`)

This crate executes RISC-V CSR instructions for RustSBI Runtime on `riscv32`
and `riscv64` targets.

## Hardware access

CSR definitions come from `riscv-csr-spec` through `csr::standard` and
`csr::vendor`. Runtime imports them alongside the hardware access traits:

```rust
use csr::{Clear, Write, standard::isa::privileged::machine::Mstatus};

// SAFETY: The trap setup runs in M-mode and owns this hart's interrupt mask.
let previous = unsafe { Mstatus::write(Clear(Mstatus::MIE)) };
let interrupts_were_enabled = previous.get(Mstatus::MIE);
```

The register definition determines which operations and operand types are available.
`Read::read()` requires `StandardRead`.
For a vendor register, the caller of `read_unchecked()` must verify the CSR number's
meaning on this core and coordinate any read effects.
`Seed` and the indirect data windows (`*ireg*`) have no `Read` implementation.
For writes, `Mip`, `Sip`, and `Vsip` accept selected bit updates only.
For example, writing a full `mip` snapshot can copy an external `SEIP` signal
into the software-pending bit.
`Mseccfg` exposes only the atomic set of `SSEED`.

| Operation | Instruction encoding | Reads CSR | Writes CSR | Rust policy |
| --- | --- | --- | --- | --- |
| Standard `Read::read()` | `csrr rd, csr` = `csrrs rd, csr, x0` | Yes | No | Safe |
| Vendor `Read::read_unchecked()` | Same read instruction and width protocol | Yes | No | Unsafe |
| Whole-value `Write::write(value)` | `csrw csr, rs1` = `csrrw x0, csr, rs1` | No | Yes | Unsafe |
| `Write::write(Set(field))` | `csrs csr, rs1` = `csrrs x0, csr, rs1` | Yes | Yes | Unsafe |
| `Write::write(Clear(field))` | `csrc csr, rs1` = `csrrc x0, csr, rs1`, or `csrrc rd, csr, rs1` for readback | Yes | Yes | Unsafe |
| `Claim::claim()` | `csrrw rd, csr, x0` | Yes | Yes, zero | Unsafe |

Here `rd` is an output register and `rs1` is a non-`x0` input register. Using `x0`
as `rd` suppresses the read for `csrrw`; `csrrs` and `csrrc` still read the CSR
when their result is discarded. A zero value in an input register still requests
a write. The architectural `x0` source suppresses writes only for `csrrs` and
`csrrc`, not `csrrw`.

Reading `*topei` reports the highest-priority eligible external interrupt.
Claiming it also clears its pending bit.
Polling `seed` requires the read/write form and consumes an entropy sample when
the result is `ES16`. A pure read of `seed` traps.
See [Zicsr](https://docs.riscv.org/reference/isa/v20260120/unpriv/zicsr.html),
[IMSIC](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html#2-1-9-top-external-interrupt-csrs-mtopei-stopei-vstopei),
and [Zkr](https://docs.riscv.org/reference/isa/v20260120/unpriv/scalar-crypto.html#crypto_scalar_seed_csr).

For read-modify-write sequences, convert the snapshot with `into_write()`,
update its fields with `set(...)`, then pass it to the register's `write` method.
`into_write` preserves every bit, so the caller must ensure that writing the
snapshot back is valid and that concurrent changes will be preserved.

RV32 uses the following word order for direct and recoverable accesses.

| RV32 protocol | Read order | Whole-write order |
| --- | --- | --- |
| Native word | Low | Low |
| `Split64` configuration | Low, high | Low, high |
| `Counter64` | High, low, high; retry if high changed | High, low |
| `Timer64` | Low, high | Low = `u32::MAX`, high, low |

On RV64, each supported register needs one CSR instruction per access.

## Recoverable access

`Read::try_read()` and `Write::try_write(operand)` return
`Result<_, AccessError>` when the instruction raises an illegal-instruction
exception. They preserve the ordinary methods' value types, operand permissions,
and RV32 access sequences. A successful typed read needs no `from_bits` call:

```rust
use csr::{Read, standard::isa::privileged::debug::Tdata1};

// SAFETY: This M-mode probe has the recovery handler, linker records, and machine
// state required by csr::recovery. The selected trigger remains stable during the read.
let data = unsafe { Tdata1::try_read() }?;
let trigger_type = data.trigger_type();
let debug_only = data.dmode();
```

All `try_*` operations are unsafe because the caller must establish the recovery
ABI and coordinate register effects. This crate emits `.trap_fixup` records;
Runtime retains them through its linker script and handles machine-origin traps
by matching the exact fault PC and accepted cause. Its handler resumes at the
record's continuation with `t2 = mcause` and `t3 = mtval`. Other general registers
are preserved except declared outputs. A handler must preserve live floating-point
and vector state if it uses those registers. Recovery changes machine trap CSRs
and trap-return status; it does not restore their pre-probe values. Finish probes
before preparing a return context, or explicitly preserve that context.
See `csr::recovery` for the record format and complete ABI.

RV32 multiword writes may fail after earlier words were written. Errors do not
roll back those writes. `machine::measure_stopped_mhpmcounter<N>()` measures the
writable width while the caller keeps accumulation stopped. A successful probe
restores the counter value. A trap during restoration can leave the probe value
in the counter.

## Access and side effects

- Instructions execute on the current hart. The scope of their effects follows
  the register: for example, `mcycle` can be shared by harts on the same core.
  An unavailable, disabled, or higher-privilege CSR can trap.
- Const-indexed definitions (`Pmpcfg<N>`, `Pmpaddr<N>`, `Mhpmcounter<N>`,
  `Mhpmevent<N>`, `Hpmcounter<N>`) live beside their register families in the spec
  crate and use the same `Read`/`Write` traits. Runtime-index operations are in
  the corresponding machine register-family files and exported from `machine`.
- The caller of `standard::aia::imsic::write_machine_registers` must exclude
  competing uses of the selector and data window, including interrupt handlers.
  The function applies writes in order and restores `miselect` on both success
  and an invalid selector. Earlier writes remain effective.
- `machine::read_medeleg_word()` and `machine::write_medeleg_word(ExceptionSet)` access
  the native-width `medeleg`. FWFT can update its delegation bits on either
  target without touching an optional RV32 upper word.
- `hypervisor::read_hedeleg_bit(exception)` reads the word containing that
  exception's delegation bit. Only RV32 exceptions 32 through 63 require a
  recoverable read of `hedelegh`.
- Recoverable event-selector adapters accept `HpmCounter`. Runtime owns optional
  high-word detection, stopped-counter verification and propagation of write errors.
- Hardware access to a `Width::Rv32Only` register, such as `Mstatush` or
  `Pmpcfg<1>`, fails to compile on RV64.

The [CSR development guidelines](../../../../docs/src/guidelines/for-development/csr-access.md)
describe how to document register families and caller obligations.

## Vendor sequences

Runtime owns state-enable initialization, capability discovery, recovery readiness,
interrupt exclusion, T-Head MAEE startup policy, and the decision to expose AIA
state to a fresh supervisor.

- T-Head `Mxstatus` and SpacemiT K1 `Msetup` both use CSR number `0x7c0`.
  The caller must select the definition for the current hart.
- Andes CCTL hardware sequences reside in `vendor::andes`, using operands
  and command encodings from `riscv-csr-spec::vendor` while requiring the caller
  to own hardware state and exclude conflicting accesses.
