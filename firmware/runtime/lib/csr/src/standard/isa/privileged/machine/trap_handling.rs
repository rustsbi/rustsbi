//! Machine trap handling.
use crate::macros::csr;
use crate::value::WriteField;
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
    /// When Sstc is implemented and `STCE` is clear, `STIP` has its behavior
    /// without Sstc; it is writable if supervisor mode is implemented.
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
    /// Sets a software-controlled pending field.
    ///
    /// This operation preserves the software-controlled portion of `SEIP`
    /// without latching an external interrupt signal into it.
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mip>.
    ///
    /// # Safety
    ///
    /// Execute in M-mode on the intended hart and coordinate pending-state
    /// changes with its interrupt-delivery owner.
    /// The selected field must be writable for the update to take effect.
    #[inline]
    pub unsafe fn set_bit<F: WriteField<InterruptPending<Write>, Value = PendingState>>() {
        let mut mask = InterruptPending::<Write>::default();
        mask.set::<F>(PendingState::Pending);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The caller coordinates pending-state updates on this hart.
        // CSRRS preserves software SEIP without latching its external signal.
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

    /// Clears a software-controlled pending field.
    ///
    /// This operation preserves the software-controlled portion of `SEIP`
    /// without latching an external interrupt signal into it.
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mip>.
    ///
    /// # Safety
    ///
    /// Execute in M-mode on the intended hart and coordinate pending-state
    /// changes with its interrupt-delivery owner.
    /// The selected field must be writable for the update to take effect.
    #[inline]
    pub unsafe fn clear_bit<F: WriteField<InterruptPending<Write>, Value = PendingState>>() {
        let mut mask = InterruptPending::<Write>::default();
        mask.set::<F>(PendingState::Pending);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The caller coordinates pending-state updates on this hart.
        // CSRRC preserves software SEIP without latching its external signal.
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
    #[inline]
    fn from(value: usize) -> Self {
        Self(value)
    }
}
