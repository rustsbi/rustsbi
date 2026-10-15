//! Hypervisor trap handling.
use crate::macros::csr;
use crate::values::{TrapInstructionBits, TrapValueBits};

csr! {
    /// Hypervisor trap value register (`htval`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#htval>.
    TrapValue { 0x643, r: TrapValueBits, w: TrapValueBits }
    /// Hypervisor transformed trap instruction (`htinst`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#htinst>.
    TrapInstruction { 0x64a, r: TrapInstructionBits, w: TrapInstructionBits }
}
