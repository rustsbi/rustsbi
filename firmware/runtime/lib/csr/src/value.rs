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

/// A typed field readable from a register snapshot.
pub trait ReadField<R> {
    type Value;
    fn read(register: R) -> Self::Value;
}

/// A typed field writable in a local register value.
pub trait WriteField<R> {
    type Value;
    fn write(register: &mut R, value: Self::Value);
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

#[cfg(test)]
mod tests {
    use super::Value;
    use crate::{
        machine,
        values::{
            Address,
            trap::{CauseValue, Exception, Interrupt, TrapCause},
            vector::{TargetVectorMode, VectorBase, VectorMode},
        },
    };

    #[test]
    fn unknown_read_encoding_survives_other_field_edits() {
        let bits = (2 << 11) | (1 << 25);
        let read = machine::StatusReadValue::from_bits(bits);
        assert_eq!(read.read::<machine::MPP>(), machine::Privilege::Unknown(2));
        let mut write = machine::StatusWriteValue::from_bits(bits);
        write.write::<machine::MIE>(machine::InterruptState::Enabled);
        assert_eq!(usize::from(write), bits | machine::MIE::MASK);
        assert_eq!(read.bits(), bits);
    }

    #[test]
    fn independent_write_value_has_typed_privilege() {
        let value = machine::StatusWriteValue::for_return(machine::TargetPrivilege::Supervisor);
        assert_eq!(value.read::<machine::MPP>(), machine::Privilege::Supervisor);
        assert_eq!(
            value.read::<machine::MIE>(),
            machine::InterruptState::Disabled
        );
    }

    #[test]
    fn vector_addresses_require_alignment_and_keep_reserved_read_modes() {
        assert!(VectorBase::new(Address::from(0x1002)).is_none());
        let base = VectorBase::new(Address::from(0x1000)).unwrap();
        let value = machine::TrapVectorWriteValue::new(base, TargetVectorMode::Vectored);
        assert_eq!(usize::from(value), 0x1001);
        let read = machine::TrapVectorReadValue::from_bits(0x1003);
        assert_eq!(read.read::<machine::MODE>(), VectorMode::Unknown(3));
        assert_eq!(usize::from(read.read::<machine::BASE>().address()), 0x1000);
    }

    #[test]
    fn unknown_causes_preserve_exception_and_interrupt_class() {
        assert_eq!(
            CauseValue::from(14).cause(),
            TrapCause::Exception(Exception::Unknown(14))
        );
        assert_eq!(
            CauseValue::from((1 << (usize::BITS - 1)) | 14).cause(),
            TrapCause::Interrupt(Interrupt::Unknown(14))
        );
        assert_eq!(
            CauseValue::from(10).cause(),
            TrapCause::Exception(Exception::VirtualSupervisorEcall)
        );
    }
}
