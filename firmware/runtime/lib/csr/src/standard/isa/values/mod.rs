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
