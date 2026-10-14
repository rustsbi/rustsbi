//! Hypervisor trap handling.
use crate::macros::csr;
use crate::values::{TrapInstructionBits, TrapValueBits};

csr! {
    /// Guest-physical address information for an HS-mode trap (`htval`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#htval>
    TrapValue { 0x643, r: TrapValueBits, w: TrapValueBits }
    /// Transformed instruction for a trap into HS-mode (`htinst`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#htinst>
    TrapInstruction { 0x64a, r: TrapInstructionBits, w: TrapInstructionBits }
}
