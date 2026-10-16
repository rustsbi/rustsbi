//! Typed CSR instructions with Runtime exception recovery.
//! Operations report illegal-instruction exceptions without emulating the CSR.
//! They add no interrupt masking or privilege changes around the instruction.
//! Recovered failures leave hardware trap CSRs describing the recovery trap.
//! Known registers can use the CSR crate directly.
//! These operations serve probes and accesses that must report failure.

use core::fmt;
use csr::{Readable, Writeable};

/// An illegal-instruction exception from a recoverable CSR access.
///
/// This error does not distinguish an absent register from denied access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IllegalInstruction;

impl fmt::Display for IllegalInstruction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("CSR access raised an illegal-instruction exception")
    }
}

macro_rules! instruction {
    ($instruction:literal, $number:expr, $value:expr) => {{
        const { assert!($number < 0x1000, "CSR number must fit in 12 bits"); }
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let mut value: usize = $value;
            let cause: usize;
            // SAFETY: The caller establishes the CSR and Runtime entry contracts.
            unsafe {
                core::arch::asm!(
                    "li t2, 0",
                    ".option push",
                    ".option norvc",
                    "2:",
                    $instruction,
                    "3:",
                    ".option pop",
                    crate::recovery::fault_site!(),
                    csr = const $number,
                    causes = const 1 << 2,
                    value = inout(reg) value,
                    // Runtime recovery returns the cause and trap value in t2/t3.
                    out("t2") cause,
                    out("t3") _,
                );
            }
            if cause == 0 { Ok(value) } else { Err(IllegalInstruction) }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = $value;
            unavailable()
        }
    }};
}

#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
fn unavailable() -> Result<usize, IllegalInstruction> {
    panic!("CSR access requires RISC-V")
}

/// Executes one CSR read, recovering an illegal-instruction exception.
///
/// # Safety
///
/// Run in M-mode after Runtime trap entry is installed, with `mscratch` zero
/// and `MPRV` clear.
/// Permit the CSR's read effects and exclude conflicting accesses.
/// Recovery does not undo successful read effects.
#[inline(never)]
pub unsafe fn read<R: Readable>() -> Result<R::Value, IllegalInstruction> {
    instruction!("csrr {value}, {csr}", R::NUMBER, 0usize).map(R::decode)
}

/// Writes one complete CSR value, recovering an illegal-instruction exception.
///
/// Success does not verify WARL readback.
///
/// # Safety
///
/// Run in M-mode after Runtime trap entry is installed, with `mscratch` zero
/// and `MPRV` clear.
/// Establish the CSR's write and access-exclusion requirements.
/// Preserve the active memory, execution, interrupt, and trap-entry invariants.
#[inline(never)]
pub unsafe fn write<R: Writeable>(value: R::Value) -> Result<(), IllegalInstruction> {
    instruction!("csrw {csr}, {value}", R::NUMBER, R::encode(value)).map(|_| ())
}

/// Executes one CSR swap, returning the previous value.
///
/// # Safety
///
/// Satisfy both [`read()`] and [`write()`] requirements for this CSR and value.
#[inline(never)]
pub unsafe fn swap<R: Readable + Writeable>(
    value: <R as Writeable>::Value,
) -> Result<<R as Readable>::Value, IllegalInstruction> {
    const {
        assert!(<R as Readable>::NUMBER == <R as Writeable>::NUMBER);
    }
    instruction!(
        "csrrw {value}, {csr}, {value}",
        <R as Readable>::NUMBER,
        R::encode(value)
    )
    .map(R::decode)
}
