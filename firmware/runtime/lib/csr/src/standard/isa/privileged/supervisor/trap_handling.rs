//! Supervisor trap handling.
use crate::macros::csr;
use crate::values::{Address, TrapValueBits, trap::CauseValue};

csr! {
    /// Supervisor exception program counter (`sepc`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/supervisor.html#sepc>.
    ExceptionPc { 0x141, r: Address,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/supervisor.html#sepc>.
        ///
        /// # Safety
        ///
        /// The eventual `sret` must resume the matching supervisor execution
        /// context.
        unsafe w: Address
    }
    /// Supervisor trap cause register (`scause`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/supervisor.html#scause>.
    Cause { 0x142, r: CauseValue, w: CauseValue }
    /// Supervisor trap value register (`stval`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/supervisor.html#stval>.
    TrapValue { 0x143, r: TrapValueBits, w: TrapValueBits }
}
