# Assembly Conventions

### Place attributes directly before the function (`asm-function-attributes`) {#asm-function-attributes}

Function attributes (`.global`, `.balign`, `.type`)
should be placed directly before the function label
and should not be indented.
Prefer `.global` over `.globl` for clarity.

```asm
.balign 4
.global foo
foo:
    mov rax, 1
    ret
```

### Prefer `.balign` over `.align` (`asm-prefer-balign`) {#asm-prefer-balign}

The `.align` directive's behavior varies across architectures —
on some it specifies a byte count,
on others a power of two.
Use `.balign` for unambiguous byte-count alignment.

```rust
// Good — unambiguous
#[unsafe(naked)]
unsafe extern "C" fn _boot_header() -> ! {
    naked_asm!(
        "j _start",
        ".word 0",
        ".balign 8",
        ".dword 0x200000",
        ".dword iend - istart",
        ".dword {RISCV_HEAD_FLAGS}",
        ".word  {RISCV_HEADER_VERSION}",
        ".word  0",
        ".dword 0",
        ".dword {RISCV_IMAGE_MAGIC}",
        ".balign 4",
        ".word  {RISCV_IMAGE_MAGIC2}",
        ".word  0",
        RISCV_HEAD_FLAGS = const RISCV_HEAD_FLAGS,
        RISCV_HEADER_VERSION = const RISCV_HEADER_VERSION,
        RISCV_IMAGE_MAGIC = const RISCV_IMAGE_MAGIC,
        RISCV_IMAGE_MAGIC2 = const RISCV_IMAGE_MAGIC2,
    );
}
```

```asm
# Bad — architecture-dependent meaning
.align 12
```

### Keep the trap entry ABI aligned (`trap-entry-abi-alignment`) {#trap-entry-abi-alignment}

The trap entry path runs before any Rust prologue,
so it owns the ABI invariants itself:
the SP it hands to Rust code must be 16-byte aligned per the RISC-V psABI,
and the `mscratch` (or equivalent scratch register) handoff
must leave the register in the state the next trap entry expects.
State the invariant in a comment at the entry symbol
so a reviewer can check the arithmetic without simulating the whole path.

```asm
# Good — the alignment and the scratch contract are stated where they hold
trap_entry:
    csrrw sp, mscratch, sp   # swap in the trap stack; mscratch now holds sp
    addi sp, sp, -16         # keep sp 16-byte aligned for the C/Rust ABI
```

### Save and restore CSRs around an access window (`save-restore-csr-around-window`) {#save-restore-csr-around-window}

Code that temporarily changes a CSR to borrow a facility
(e.g. setting `mstatus.MPRV` to access guest memory,
reprogramming an indirect-select register like `miselect`)
must restore the original value on every exit path —
including error paths —
before any code that assumes the CSR's steady-state value can run.
Pair the restore with the set in the same function,
so the window is visible in one place.

The RISC-V branch of
[`access_byte!` in Runtime's memory-access implementation](https://github.com/rustsbi/rustsbi/blob/ebc7190fb630611fa9504f9f71089ce4b81c6c0e/firmware/runtime/src/trap/memory.rs#L16-L61)
saves `mstatus` before enabling `MPRV` for one load or store.
The following excerpt uses the macro's `$instruction` argument for that access:

```rust
let mut data: usize = $data;
let cause: usize;
let tval: usize;
// SAFETY:
// 1. access_status restricts effective privilege to the original S/U context.
// 2. A nested trap sets MPP=Machine, so entry can use its stack under MPRV.
// 3. Entry clears MPRV before Rust and preserves the saved mstatus operand.
// 4. Success and recovery both restore mstatus before returning to Rust.
unsafe {
    core::arch::asm!(
        "csrrci {saved_status}, {status_csr}, 8", // Clear mstatus.MIE.
        "not {access_status}, {access_mask}",
        "and {access_status}, {saved_status}, {access_status}",
        "or {access_status}, {access_status}, {requested_status}",
        "li t2, 0",
        "li t3, 0",
        ".option push",
        ".option norvc",
        "csrw {status_csr}, {access_status}",
        "2:",
        $instruction,
        "3:",
        "csrw {status_csr}, {saved_status}",
        ".option pop",
        crate::recovery::fault_site!(),
        status_csr = const machine::Status::NUMBER,
        causes = const $causes,
        data = inout(reg) data,
        address = in(reg) $address,
        requested_status = in(reg) $status,
        // Replace MIE, MPP, MPRV, SUM, and MXR; preserve other fields.
        access_mask = in(reg) (1usize << 3) | (3 << 11) | (7 << 17),
        saved_status = out(reg) _,
        access_status = out(reg) _,
        out("t2") cause,
        out("t3") tval,
    );
}
if cause == 0 {
    Ok(data)
} else {
    let TrapCause::Exception(cause) = CauseValue::from(cause).cause() else {
        unreachable!("recovery accepts synchronous memory faults only")
    };
    Err(Error::Fault(Fault { cause, address: tval.into() }))
}
```

The [`fault_site!` record](https://github.com/rustsbi/rustsbi/blob/ebc7190fb630611fa9504f9f71089ce4b81c6c0e/firmware/runtime/src/recovery.rs#L50-L65)
maps the access at label `2` to the continuation at label `3`.
On an accepted memory fault,
[trap dispatch](https://github.com/rustsbi/rustsbi/blob/ebc7190fb630611fa9504f9f71089ce4b81c6c0e/firmware/runtime/src/trap/dispatch.rs#L48-L57)
sets the saved `t2` and `t3` to the exception cause and fault address,
then resumes at label `3`.
Both successful access and fault recovery therefore execute the same
`csrw` that restores `mstatus`
before Rust returns `Ok(data)` or `Err(Error::Fault(...))`.
