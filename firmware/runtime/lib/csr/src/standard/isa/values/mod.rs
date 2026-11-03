//! Operands shared by CSR definitions.
//!
//! See [`mepc`](https://docs.riscv.org/reference/isa/priv/machine.html#mepc),
//! [`mtval`](https://docs.riscv.org/reference/isa/priv/machine.html#mtval),
//! and [`mtinst`](https://docs.riscv.org/reference/isa/priv/hypervisor.html#mtinst)
//! for the address and trap-value operands.
pub mod counter;
pub mod trap;
pub mod vector;
pub mod xlen;

use crate::macros::csr;

csr! {
    pub struct Address(usize);
    pub struct TrapValueBits(usize);
    pub struct TrapInstructionBits(usize);
    /// A word of a state-enable register.
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/smstateen.html>.
    pub struct StateEnableSet(usize);
}

impl StateEnableSet {
    /// Selects no state-enable bits in this word.
    #[inline]
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Returns the lower-register enable bit in the low word.
    ///
    /// On RV32 the bit is in the high word, so this returns an empty set.
    #[inline]
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

    /// Returns the lower-register enable bit in the high word on RV32.
    #[inline]
    pub const fn lower_registers_high() -> Self {
        Self(1usize << 31)
    }
}

impl From<usize> for Address {
    #[inline]
    fn from(address: usize) -> Self {
        Self(address)
    }
}

impl From<usize> for TrapValueBits {
    #[inline]
    fn from(value: usize) -> Self {
        Self(value)
    }
}

impl From<usize> for TrapInstructionBits {
    #[inline]
    fn from(value: usize) -> Self {
        Self(value)
    }
}
