//! Conversion between a declared CSR value and its instruction operand.

pub(crate) trait Value: Copy {
    const BITS: u32;
    fn from_bits(bits: usize) -> Self;
    fn bits(self) -> usize;
}

/// A CSR with a typed read value.
pub trait Readable {
    /// Architectural CSR address.
    const NUMBER: u16;
    /// Immutable register snapshot.
    type Value;
    /// Decodes the instruction result, preserving unknown field encodings.
    fn decode(bits: usize) -> Self::Value;
}

/// A CSR with a typed write value.
pub trait Writeable {
    /// Architectural CSR address.
    const NUMBER: u16;
    /// Complete register write value.
    type Value;
    /// Encodes a typed write value without accessing hardware.
    fn encode(value: Self::Value) -> usize;
}

/// Reads the selected CSR's instruction operand.
///
/// # Safety
///
/// Permit the register's read effects and exclude conflicting accesses.
#[inline]
pub(crate) unsafe fn read<const CSR: u16>() -> usize {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    {
        let bits: usize;
        // SAFETY: Each caller establishes the register's read requirements.
        unsafe {
            core::arch::asm!("csrr {bits}, {csr}", bits = out(reg) bits,
            csr = const CSR, options(nostack));
        }
        bits
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    panic!("CSR access requires RISC-V")
}

/// Writes an encoded operand to the selected CSR.
///
/// # Safety
///
/// Supply an operand permitted by the register's write contract.
/// Exclude conflicting accesses and preserve the execution, memory, interrupt,
/// and trap-handler invariants the write affects.
#[inline]
pub(crate) unsafe fn write<const CSR: u16>(bits: usize) {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    // SAFETY: Each caller establishes its particular register's requirements.
    unsafe {
        core::arch::asm!("csrw {csr}, {bits}", csr = const CSR, bits = in(reg) bits, options(nostack));
    }
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    {
        let _ = bits;
        panic!("CSR access requires RISC-V");
    }
}
