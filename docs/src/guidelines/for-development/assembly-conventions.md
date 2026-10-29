# Assembly Conventions

### Place attributes directly before the function (`asm-function-attributes`) {#asm-function-attributes}

Function attributes (`.global`, `.balign`, `.type`)
should be placed directly before the function label
and should not be indented.
Prefer `.global` over `.globl` for clarity.

```rust
#[unsafe(naked)]
pub(crate) unsafe extern "C" fn warm_entry() -> ! {
    core::arch::naked_asm!(
        ".balign 4",
        "la a0, {initialize}",
        "tail {entry}",
        initialize = sym initialize,
        entry = sym boot::k1_warm_entry,
    )
}
```

### Prefer `.balign` over `.align` (`asm-prefer-balign`) {#asm-prefer-balign}

The `.align` directive's behavior varies across architectures —
on some it specifies a byte count,
on others a power of two.
Use `.balign` for unambiguous byte-count alignment.

```rust
# Good — unambiguous
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

See also:
PR [#2368](https://github.com/asterinas/asterinas/pull/2368).
