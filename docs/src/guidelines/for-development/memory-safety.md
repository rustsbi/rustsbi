# Memory Safety

### Justify every use of `unsafe` (`justify-unsafe-use`) {#justify-unsafe-use}

Every `unsafe` block must have a preceding `// SAFETY:` comment
that justifies why the operation is sound.
For multi-condition invariants,
use a numbered list:

```rust
// SAFETY:
// 1. access_status restricts effective privilege to the original S/U context.
// 2. A nested trap sets MPP=Machine, so entry can use its stack under MPRV.
// 3. Entry clears MPRV before Rust and preserves the saved mstatus operand.
// 4. Success and recovery both restore mstatus before returning to Rust.
unsafe {
    core::arch::asm!(
        // ...
    )
}
```

### Document safety conditions (`document-safety-conds`) {#document-safety-conds}

All `unsafe` functions and traits
must include a `# Safety` section in their doc comments
describing the conditions, properties, or invariants that callers must uphold.
State exactly what the caller must guarantee —
not implementation details or side effects.

```rust
/// Reads an indirectly accessed machine-level interrupt-file register.
///
/// # Safety
///
/// The current hart must implement Smaia, the caller must be permitted to access
/// M-mode CSRs, and `reg_id` must select an implemented register for the current
/// XLEN. Otherwise, the indirect CSR access may raise an illegal-instruction
/// exception.
#[inline]
pub(super) unsafe fn read_ind(reg_id: usize) -> usize {
    interrupt::free(|| unsafe {
        let previous = miselect::read();
        miselect::write(Miselect::from_bits(reg_id));
        let value = mireg::read().bits();
        miselect::write(previous);
        value
    })
}
```

### Reason about safety at the module boundary (`module-boundary-safety`) {#module-boundary-safety}

The safety of an `unsafe` block
depends on ALL code that can access the same private state.
Encapsulate unsafe abstractions
in the smallest possible module
to minimize the "audit surface."
Any code in the same module
that can modify relied-upon fields
is part of the safety argument.

```rust
// Good — small, focused module limits the audit surface
/// M-mode accessors for the machine-level interrupt file.
mod machine {
    use crate::register::{
        mireg::{self, Mireg},
        miselect::{self, Miselect},
    };
    use riscv::interrupt::machine as interrupt;

    /// Reads an indirectly accessed machine-level interrupt-file register.
    ///
    /// # Safety
    ///
    /// The current hart must implement Smaia, the caller must be permitted to access
    /// M-mode CSRs, and `reg_id` must select an implemented register for the current
    /// XLEN. Otherwise, the indirect CSR access may raise an illegal-instruction
    /// exception.
    #[inline]
    pub(super) unsafe fn read_ind(reg_id: usize) -> usize {
        interrupt::free(|| unsafe {
            let previous = miselect::read();
            miselect::write(Miselect::from_bits(reg_id));
            let value = mireg::read().bits();
            miselect::write(previous);
            value
        })
    }
    // ...
}
```
