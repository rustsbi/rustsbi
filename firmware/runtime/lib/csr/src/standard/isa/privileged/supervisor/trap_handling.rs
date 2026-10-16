//! Supervisor trap handling.
use crate::macros::csr;
use crate::values::{Address, TrapValueBits, trap::CauseValue};

csr! {
    /// Supervisor trap return address (`sepc`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/supervisor.html#sepc>
    ExceptionPc { 0x141, r: Address,
        /// See <https://docs.riscv.org/reference/isa/priv/supervisor.html#sepc>.
        ///
        /// # Safety
        ///
        /// The eventual `sret` must resume the matching supervisor execution context.
        unsafe w: Address
    }
    /// Supervisor interrupt or exception cause (`scause`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/supervisor.html#scause>
    Cause { 0x142, r: CauseValue, w: CauseValue }
    /// Supervisor trap-specific address or instruction bits (`stval`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/supervisor.html#stval>
    TrapValue { 0x143, r: TrapValueBits, w: TrapValueBits }
}
