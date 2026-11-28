# `rustsbi-csr` (`csr`)

Target-specific RISC-V CSR hardware instruction execution layer for RustSBI
runtime (`publish = false`, `riscv32` and `riscv64` targets only). Attempting
to build this crate on non-RISC-V targets fails compilation immediately.

## Hardware access

CSR definitions come from `riscv-csr-spec` through `csr::standard` and
`csr::vendor`. Runtime imports them alongside the hardware access traits:

```rust
use csr::{Clear, Write, standard::isa::privileged::machine::Mstatus};

// SAFETY: The trap setup runs in M-mode and owns this hart's interrupt mask.
let previous = unsafe { Mstatus::write(Clear(Mstatus::MIE)) };
let interrupts_were_enabled = previous.get(Mstatus::MIE);
```

The hardware execution layer provides three access traits. They execute CSR
instructions from RISC-V's Zicsr extension and the required RV32 multiword
access protocols:

- `Read::read()`: reads each word using Zicsr `csrr` (`csrrs rd, csr, x0`).
  On RV32, a `Counter64` read retries if the upper word changes during counting.
  Vendor CSRs use `unsafe Read::read_unchecked()` with caller-established hardware
  applicability and read-effect requirements. Both methods are withheld on `Seed`
  (`Zkr`, where pure `csrr` traps) and indirect windows (`*ireg*`).
- `Write::write(operand)`: unified write dispatching whole-register `csrw`
  (or RV32 `u32::MAX / hi / lo` three-write protocol on `Sstc` `*timecmp`),
  `Set(...)` (`csrs`), and `Clear(...)` (`csrc`, or atomic `csrrc` readback on
  `Mstatus`/`Sstatus`). Whole-register `csrw` is withheld on `Mip`/`Sip`/`Vsip`
  (to avoid latching external `SEIP`) and `Mseccfg` (to protect sticky lock
  bits); `Set`/`Clear` are restricted to CSRs with genuine atomic-update needs.
- `Claim::claim()`: atomic `csrrw rd, csr, x0` restricted to `*topei`
  interrupt claims (`AIA IMSIC`) and `Seed` entropy polling (`Zkr`).

The instructions have distinct read and write effects:

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

The write of a claim/poll triggers the CSR-specific action. Pure reads of
`*topei` only observe interrupts. Polling `seed` requires the read/write form;
pure reads trap. Its consumed entropy and IMSIC pending-bit changes belong to
the write operation. See [Zicsr](https://docs.riscv.org/reference/isa/v20260120/unpriv/zicsr.html),
[IMSIC](https://docs.riscv.org/reference/aia/v1.0/IMSIC.html#2-1-9-top-external-interrupt-csrs-mtopei-stopei-vstopei),
and [Zkr](https://docs.riscv.org/reference/isa/v20260120/unpriv/scalar-crypto.html#crypto_scalar_seed_csr).

Rust safety is this crate's interface policy. It provides safe standard reads and
requires `unsafe` for vendor reads, every write and every claim.
`Read::read()` requires `StandardRead`, implemented by standard `r` declarations.
The specification's operand bounds
remain enforced inside an unsafe block; the caller establishes the documented
hardware state and synchronization conditions. `try_read` and `try_write` also
require `unsafe` because their caller must establish the recovery ABI.

For read-modify-write sequences, convert the snapshot with `into_write()`,
update its fields with `set(...)`, then pass it to `unsafe { write(...) }`.
`ReadMode` and `WriteMode` identify these local value states; they execute no
instructions and do not classify Rust safety. `from_bits` decodes a stored raw
snapshot, while `read` obtains one directly from hardware. Preparing a value with
`into_write` does not establish the hardware preconditions for writing it.

In `instruction.rs`, reads select the target width first. Writes select the
permitted operation, with whole-value word order kept in `write_whole`:

| RV32 protocol | Read order | Whole-write order |
| --- | --- | --- |
| Native word | Low | Low |
| `Split64` configuration | Low, high | Low, high |
| `Counter64` | High, low, high; retry if high changed | High, low |
| `Timer64` | Low, high | Low = `u32::MAX`, high, low |

RV64 accesses use one CSR instruction. The const recovery parameter shares these
protocols between ordinary and `try_*` methods. Representative release builds on
both targets eliminate ordinary-path `Result` handling and width/operation
dispatch; recovery retains status initialization, result checking and records.

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
roll back those writes. `machine::probe_mhpmcounter<N>()` measures the writable
width while the counter is stopped. It uses the counter's `mcountinhibit` bit
when available and otherwise verifies that the event selector accepts zero.
A successful probe restores the counter and its controls. A trap during restoration
can leave the counter value, inhibit bit, or event selector changed.

## Access and side effects

- Instructions execute on the current hart. The scope of their effects follows
  the register: for example, `mcycle` can be shared by harts on the same core.
  An unavailable, disabled, or higher-privilege CSR can trap.
- Const-indexed definitions (`Pmpcfg<N>`, `Pmpaddr<N>`, `Mhpmcounter<N>`,
  `Mhpmevent<N>`, `Hpmcounter<N>`) live beside their register families in the spec
  crate and use the same `Read`/`Write` traits. Runtime-index operations are in
  the corresponding machine register-family files and exported from `machine`.
- `standard::aia::imsic::write_machine_register` accepts `ImsicRegister`, validates
  its encoding for the current XLEN, then exchanges `miselect`, writes `mireg`,
  and restores the selector in one assembly window. Invalid encodings return
  `None` before accessing hardware. Runtime owns accessibility and IRQ exclusion;
  a hardware trap can interrupt restoration.
- `machine::read_medeleg_word()` and `machine::write_medeleg_word(usize)` access
  the native-width `medeleg`. FWFT can update its delegation bits on either
  target without touching an optional RV32 upper word.
- `hypervisor::read_hedeleg_bit(exception)` reads the word containing that
  exception's delegation bit. Only RV32 exceptions 32 through 63 require a
  recoverable read of `hedelegh`.
- Recoverable event-selector adapters accept `HpmCounter`. Runtime owns optional
  high-word detection, stopped-counter verification and propagation of write errors.
- Accessing an `RV32Only` CSR (`Width::Rv32Only`, e.g. `Mstatush`) from
  `Read`, `Write`, or `Claim` on RV64 fails compilation via
  const assertion.

## Sources and safety

Every hardware operation and value definition must remain traceable to the
direct specification links in its Rust doc comments and fulfill distinct Rust
soundness requirements:

- A hardware specification reference documents architectural behavior; it does
  not establish Rust memory or concurrency safety.
- Every `unsafe` function must document all caller obligations in `# Safety`.
- Every `unsafe { ... }` call site must carry a `// SAFETY:` justification
  explaining why those invariants hold at that site.

## Vendor sequences

Runtime owns state-enable initialization, capability discovery, recovery readiness,
interrupt exclusion, T-Head MAEE startup policy, and the decision to expose AIA
state to a fresh supervisor.

- Vendor CSRs (`csr::vendor::thead`, `csr::vendor::andes`,
  `csr::vendor::spacemit::k1`) use the same `Read`/`Write` traits in
  isolated vendor namespaces (`0x7c0` never collides across vendors).
  Their reads require `unsafe read_unchecked()`; register capabilities and width
  restrictions remain enforced.
- Andes CCTL hardware sequences reside in `vendor::andes`, using operands
  and command encodings from `riscv-csr-spec::vendor` while requiring the caller
  to own hardware state and exclude conflicting accesses.
