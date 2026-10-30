//! Machine trap handling.
use crate::macros::csr;
use crate::{Write};
use crate::values::{Address, TrapInstructionBits, TrapValueBits, trap::CauseValue};

csr! {
    /// Machine trap scratch register (`mscratch`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mscratch>.
    Scratch { 0x340, r,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/machine.html#mscratch>.
        ///
        /// # Safety
        ///
        /// Preserve the active trap handler's scratch and stack protocol.
        unsafe w
    }
    /// Machine exception program counter (`mepc`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mepc>.
    ExceptionPc { 0x341, r: Address,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mepc>.
        ///
        /// # Safety
        ///
        /// The eventual `mret` must resume a valid execution context at this
        /// address.
        unsafe w: Address
    }
    /// Machine trap cause register (`mcause`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mcause>.
    Cause { 0x342, r: CauseValue, w: CauseValue }
    /// Machine trap value register (`mtval`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mtval>.
    TrapValue { 0x343, r: TrapValueBits, w: TrapValueBits }
    /// Machine interrupt-pending register (`mip`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mip>.
    /// Use `set_bit` or `clear_bit` to update software-controlled pending bits.
    /// Generic whole-register writes and read/modify/write are unavailable.
    /// When `menvcfg.STCE` is set, `STIP` reflects `stimecmp` and is read-only.
    /// Clearing `STCE` restores software control of `STIP`.
    /// See <https://docs.riscv.org/reference/isa/priv/sstc.html>.
    InterruptPending { 0x344, r }
        SSIP[1] { rw: PendingState { Clear = 0, Pending = 1 } }
        STIP[5] { rw: PendingState }
    /// Machine transformed trap instruction (`mtinst`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#mtinst>.
    TrapInstruction { 0x34a, r: TrapInstructionBits, w: TrapInstructionBits }
    /// Additional machine trap value (`mtval2`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#mtval2>.
    TrapValue2 { 0x34b, r: TrapValueBits, w: TrapValueBits }
}

impl InterruptPending {
    /// Sets a software-controlled pending field with one CSRRS instruction.
    ///
    /// The SEIP read value combines software state with the external signal.
    /// CSRRS excludes the external signal from the value used for the update.
    /// A separate read/write can latch that signal into software pending state.
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mip>.
    ///
    /// # Safety
    ///
    /// Execute in M-mode on the intended hart and coordinate pending-state
    /// changes with its interrupt-delivery owner.
    #[inline]
    pub unsafe fn set_bit<
        F: crate::value::WriteField<InterruptPending<Write>, Value = PendingState>,
    >() {
        let mut mask = InterruptPending::<Write>::default();
        mask.write_field::<F>(PendingState::Pending);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: these instructions update software-controlled pending bits.
        // They preserve the distinction between software SEIP and its external signal.
        unsafe {
            core::arch::asm!(
                "csrs {csr}, {mask}",
                csr = const Self::NUMBER,
                mask = in(reg) usize::from(mask),
                options(nostack),
            );
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = mask;
            unimplemented!("CSR access requires RISC-V");
        }
    }

    /// Clears a software-controlled pending field with one CSRRC instruction.
    ///
    /// For SEIP, CSRRC updates only its software state, excluding the external
    /// signal.
    /// Clearing software SEIP does not clear an asserted external signal.
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mip>.
    ///
    /// # Safety
    ///
    /// Execute in M-mode on the intended hart and coordinate pending-state
    /// changes with its interrupt-delivery owner.
    #[inline]
    pub unsafe fn clear_bit<
        F: crate::value::WriteField<InterruptPending<Write>, Value = PendingState>,
    >() {
        let mut mask = InterruptPending::<Write>::default();
        mask.write_field::<F>(PendingState::Pending);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: CSRRC changes only the software-controlled pending bits and
        // excludes the external SEIP signal from the update calculation.
        unsafe {
            core::arch::asm!(
                "csrc {csr}, {mask}",
                csr = const Self::NUMBER,
                mask = in(reg) usize::from(mask),
                options(nostack),
            );
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = mask;
            unimplemented!("CSR access requires RISC-V");
        }
    }
}

impl From<usize> for Scratch {
    fn from(value: usize) -> Self {
        Self(value)
    }
}
