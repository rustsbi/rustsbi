//! Machine trap handling.
use crate::macros::csr;
use crate::values::{Address, TrapInstructionBits, TrapValueBits, trap::CauseValue};

csr! {
    pub struct ScratchValue(usize);

    /// Machine trap-handler scratch storage (`mscratch`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mscratch>
    Scratch { 0x340, r: ScratchValue,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mscratch>.
        ///
        /// # Safety
        ///
        /// Preserve the active trap handler's scratch and stack protocol.
        unsafe w: ScratchValue
    }
    /// Machine trap return address (`mepc`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mepc>
    ExceptionPc { 0x341, r: Address,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mepc>.
        ///
        /// # Safety
        ///
        /// The eventual `mret` must resume a valid execution context at this address.
        unsafe w: Address
    }
    /// Machine interrupt or exception cause (`mcause`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mcause>
    Cause { 0x342, r: CauseValue, w: CauseValue }
    /// Machine trap-specific address or instruction bits (`mtval`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mtval>
    TrapValue { 0x343, r: TrapValueBits, w: TrapValueBits }
    /// Pending machine interrupts (`mip`).
    ///
    /// Updates require CSRRS/CSRRC bit operations: a full read-modify-write can
    /// latch the hardware-ORed SEIP signal into the software-writable SEIP bit.
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mip>
    InterruptPending { 0x344, r }
        SSIP[1] { rw: PendingState { Clear = 0, Pending = 1 } }
        STIP[5] { rw: PendingState }
    /// Transformed instruction for a trap into M-mode (`mtinst`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#mtinst>
    TrapInstruction { 0x34a, r: TrapInstructionBits, w: TrapInstructionBits }
    /// Additional guest-physical address information (`mtval2`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#mtval2>
    TrapValue2 { 0x34b, r: TrapValueBits, w: TrapValueBits }
}

impl From<usize> for ScratchValue {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
