//! Machine trap setup.
use crate::macros::csr;
use crate::value::WriteField;
use crate::{Read, Write};
use crate::values::{
    counter::CounterSet,
    trap::{ExceptionSet, InterruptSet},
    vector::{TargetVectorMode, VectorBase, VectorMode},
};

csr! {
    /// Machine status register (`mstatus`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mstatus>.
    Status { 0x300, r,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/machine.html#mstatus>.
        ///
        /// # Safety
        ///
        /// Preserve address-space, extension-state, and interrupt-exclusion
        /// invariants.
        /// `MPRV` selects the effective privilege of subsequent Rust loads and
        /// stores.
        unsafe w
    }
        SIE[1] { rw: Interrupt { Disabled = 0, Enabled = 1 } }
        MIE[3] { rw: Interrupt }
        SPIE[5] { rw: Interrupt }
        UBE[6] { rw: Endianness { Little = 0, Big = 1 } }
        MPIE[7] { rw: Interrupt }
        SPP[8] { rw: SupervisorPrivilege { User = 0, Supervisor = 1 } }
        MPP[11..=12] {
            r: Privilege { User = 0, Supervisor = 1, Machine = 3 },
            w: TargetPrivilege { User = 0, Supervisor = 1, Machine = 3 }
        }
        MPRV[17] { rw: AccessPrivilege { Current = 0, Previous = 1 } }
        SUM[18] { rw: UserMemoryAccess { Denied = 0, Allowed = 1 } }
        MXR[19] { rw: ExecutableReadAccess { Disabled = 0, Enabled = 1 } }
        #[cfg(target_pointer_width = "64")]
        SBE[36] { rw: Endianness }
        #[cfg(target_pointer_width = "64")]
        GVA[38] { rw: GuestAddress }
        #[cfg(target_pointer_width = "64")]
        MPV[39] { rw: Virtualization }
        #[cfg(target_pointer_width = "64")]
        MDT[42] { rw: DoubleTrapState }

    /// Machine ISA register (`misa`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#misa>.
    Isa { 0x301, r,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#misa>.
        ///
        /// # Safety
        ///
        /// The selected extensions and instruction alignment must support all
        /// live code, register state, and interrupt handlers on this hart.
        unsafe w
    }
    /// Machine exception delegation register (`medeleg`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#medeleg>.
    ExceptionDelegation { 0x302, r: ExceptionSet,
        /// Updates exception routing.
        ///
        /// # Safety
        ///
        /// Preserve active trap handling and the execution context of live code.
        unsafe w: ExceptionSet
    }
    /// Machine interrupt delegation register (`mideleg`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mideleg>.
    InterruptDelegation { 0x303, r: InterruptSet, w: InterruptSet }
    /// Machine interrupt-enable register (`mie`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mie>.
    InterruptEnable { 0x304, r,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mie>.
        ///
        /// # Safety
        ///
        /// Install valid handlers for enabled sources.
        /// Permit their re-entry without violating active exclusion or memory
        /// invariants.
        unsafe w: InterruptSet
    }
        MSIE[3] { rw: Interrupt }
        MTIE[7] { rw: Interrupt }
        MEIE[11] { rw: Interrupt }
    /// Machine trap-vector register (`mtvec`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mtvec>.
    TrapVector { 0x305, r,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mtvec>.
        ///
        /// # Safety
        ///
        /// The vector must preserve Rust execution state, with a valid trap
        /// stack.
        unsafe w
    }
        MODE[0..=1] { r: VectorMode, w: TargetVectorMode }
        #[cfg(target_pointer_width = "64")]
        BASE[2..=63] { rw: VectorBase }
        #[cfg(target_pointer_width = "32")]
        BASE[2..=31] { rw: VectorBase }
    /// Machine counter-enable register (`mcounteren`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mcounteren>.
    CounterEnable { 0x306, r: CounterSet, w: CounterSet }
    /// Upper half of machine status on RV32 (`mstatush`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mstatush>.
    StatusHigh { 0x310, r,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/machine.html#mstatush>.
        ///
        /// # Safety
        ///
        /// Preserve address translation, endianness, and trap exclusion on this
        /// hart.
        unsafe w
    }
        SBEH[4] { rw: Endianness }
        GVAH[6] { rw: GuestAddress { Other = 0, GuestVirtual = 1 } }
        MPVH[7] { rw: Virtualization { Off = 0, On = 1 } }
        MDTH[10] { rw: DoubleTrapState { Clear = 0, Set = 1 } }
}

impl Status {
    /// Sets an interrupt field.
    ///
    /// # Safety
    ///
    /// Execute in M-mode on the intended hart. Satisfy this register's write
    /// requirements and permit interrupt reentry.
    #[inline]
    pub unsafe fn set_bit<F: WriteField<Status<Write>, Value = Interrupt>>() {
        let mut mask = Status::<Write>::from_raw(0);
        mask.set::<F>(Interrupt::Enabled);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The caller establishes current-hart access and write safety.
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

    /// Atomically clears an interrupt field and returns the previous value.
    ///
    /// # Safety
    ///
    /// Execute in M-mode on the intended hart and satisfy this register's write
    /// requirements.
    #[inline]
    pub unsafe fn clear_bit<F: WriteField<Status<Write>, Value = Interrupt>>() -> Status<Read> {
        let mut mask = Status::<Write>::from_raw(0);
        mask.set::<F>(Interrupt::Enabled);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let previous: usize;
            // SAFETY: The caller establishes current-hart access and write safety.
            unsafe {
                core::arch::asm!(
                    "csrrc {previous}, {csr}, {mask}",
                    previous = out(reg) previous,
                    csr = const Self::NUMBER,
                    mask = in(reg) usize::from(mask),
                    options(nostack),
                );
            }
            Status::<Read>::from_raw(previous)
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = mask;
            unimplemented!("CSR access requires RISC-V");
        }
    }
}

impl From<bool> for Virtualization {
    #[inline]
    fn from(virtualized: bool) -> Self {
        if virtualized { Self::On } else { Self::Off }
    }
}

impl Isa {
    /// Reports whether `misa` advertises an extension by its uppercase letter.
    ///
    /// Returns `false` for characters outside `A` through `Z`.
    /// A clear bit does not necessarily prove that an extension is absent.
    /// See <https://docs.riscv.org/reference/isa/priv/machine.html#misa>.
    #[inline]
    pub const fn has_extension(self, extension: char) -> bool {
        let code = extension as u32;
        code >= 'A' as u32 && code <= 'Z' as u32 && self.0 & (1 << (code - 'A' as u32)) != 0
    }
}
