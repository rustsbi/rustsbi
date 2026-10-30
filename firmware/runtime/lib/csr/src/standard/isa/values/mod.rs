//! Operands shared by CSR definitions.
pub mod counter;
pub mod trap;
pub mod vector;
pub mod xlen;

use crate::macros::csr;

csr! {
    pub struct Address(usize);
    pub struct TrapValueBits(usize);
    pub struct TrapInstructionBits(usize);
    pub struct StateEnableSet(usize);
}

impl StateEnableSet {
    /// Requests zero for every writable state-enable bit in this word.
    pub const fn closed() -> Self {
        Self(0)
    }

    /// Permits the corresponding lower-level state-enable register.
    pub const fn lower_registers() -> Self {
        #[cfg(target_pointer_width = "64")]
        {
            Self(1usize << 63)
        }
        #[cfg(target_pointer_width = "32")]
        {
            Self(0)
        }
    }

    /// Upper word of the lower-register permission on RV32.
    #[cfg(target_pointer_width = "32")]
    pub const fn lower_registers_high() -> Self {
        Self(1usize << 31)
    }
}

impl From<usize> for Address {
    fn from(address: usize) -> Self {
        Self(address)
    }
}

impl From<usize> for TrapValueBits {
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<usize> for TrapInstructionBits {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
