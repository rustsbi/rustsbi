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

```rust
// Good — the window opens and closes in one scope, error paths included
interrupt::free(|| unsafe {
    let previous = miselect::read();
    miselect::write(Miselect::from_bits(reg_id));
    let value = mireg::read().bits();
    miselect::write(previous);
    value
})
```
