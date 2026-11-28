//! Trap-vector dispatch modes and aligned base addresses.
//!
//! [`mtvec`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-7-machine-trap-vector-base-address-mtvec-register)
//! and [`stvec`](https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#11-1-1-2-supervisor-trap-vector-base-address-stvec-register)
//! encode the dispatch mode in the low two bits of the register.

use super::address::Address;
use crate::{ReadFieldValue, WriteFieldValue};

field_enum! {
    /// Trap vector dispatch mode encoded in `*tvec.MODE`.
    pub enum VectorMode {
        /// All traps set `pc` to `BASE`.
        Direct = 0,
        /// Exceptions use `BASE` and interrupts use `BASE + 4 * cause`.
        Vectored = 1,
    }
}

/// Four-byte-aligned trap entry base address stored in `*tvec[XLEN-1:2]`.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VectorBase(usize);

impl VectorBase {
    /// Returns a trap-vector base if `address` is four-byte aligned.
    ///
    /// Vectored mode may impose stricter platform alignment requirements.
    #[inline(always)]
    pub const fn new(address: Address) -> Option<Self> {
        if address.0 & 0b11 == 0 {
            Some(Self(address.0))
        } else {
            None
        }
    }

    /// Returns the byte address of the trap-vector base.
    #[inline(always)]
    pub const fn address(self) -> Address {
        Address(self.0)
    }
}

impl ReadFieldValue for VectorBase {
    #[inline(always)]
    fn from_field(raw: usize) -> Self {
        Self(raw << 2)
    }
}
impl WriteFieldValue for VectorBase {
    #[inline(always)]
    fn into_field(self) -> usize {
        self.0 >> 2
    }
}
