//! Supervisor trap setup.
use super::super::machine::{Interrupt, SupervisorPrivilege};
use crate::macros::csr;
use crate::value::WriteField;
use crate::{Read, Write};
use crate::values::{
    counter::CounterSet,
    trap::InterruptSet,
    vector::{TargetVectorMode, VectorBase, VectorMode},
};

csr! {
    /// Supervisor status register (`sstatus`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/supervisor.html#sstatus>.
    Status { 0x100, r,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/supervisor.html#sstatus>.
        ///
        /// # Safety
        ///
        /// Preserve interrupt exclusion, address-space and extension-state
        /// invariants.
        unsafe w
    }
        SIE[1] { rw: Interrupt }
        SPIE[5] { rw: Interrupt }
        SPP[8] { rw: SupervisorPrivilege }
    /// Supervisor interrupt-enable register (`sie`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/supervisor.html#sie>.
    InterruptEnable { 0x104, r,
        /// See <https://docs.riscv.org/reference/isa/priv/supervisor.html#sie>.
        ///
        /// # Safety
        ///
        /// Enabled interrupt handlers must preserve the interrupted execution
        /// state.
        unsafe w: InterruptSet
    }
        SSIE[1] { rw: Interrupt }
        STIE[5] { rw: Interrupt }
    /// Supervisor trap-vector register (`stvec`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/supervisor.html#stvec>.
    TrapVector { 0x105, r,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/supervisor.html#stvec>.
        ///
        /// # Safety
        ///
        /// Supply a valid supervisor trap entry and its stack protocol.
        unsafe w
    }
        MODE[0..=1] { r: VectorMode, w: TargetVectorMode }
        #[cfg(target_pointer_width = "64")]
        BASE[2..=63] { rw: VectorBase }
        #[cfg(target_pointer_width = "32")]
        BASE[2..=31] { rw: VectorBase }
    /// Supervisor counter-enable register (`scounteren`).
    ///
    /// See
    /// <https://docs.riscv.org/reference/isa/priv/supervisor.html#scounteren>.
    CounterEnable { 0x106, r: CounterSet, w: CounterSet }
}

impl Status {
    /// Atomically clears an interrupt field, returning its previous state.
    ///
    /// # Safety
    ///
    /// Execute on the intended hart with supervisor CSR access and preserve
    /// the current interrupt-exclusion protocol.
    pub unsafe fn clear_bit<F: WriteField<Status<Write>, Value = Interrupt>>() -> Status<Read> {
        let mut mask = Status::<Write>::default();
        mask.set::<F>(Interrupt::Enabled);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let previous: usize;
            // SAFETY: The caller permits the current hart's interrupt update.
            unsafe {
                core::arch::asm!("csrrc {old}, {csr}, {mask}", old = out(reg) previous,
                csr = const Self::NUMBER, mask = in(reg) usize::from(mask), options(nostack));
            }
            Status::<Read>::from_raw(previous)
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = mask;
            unimplemented!("CSR access requires RISC-V")
        }
    }

    /// Atomically sets an interrupt field.
    ///
    /// # Safety
    ///
    /// Execute on the intended hart with supervisor CSR access and ensure
    /// enabled interrupt handlers can run safely.
    pub unsafe fn set_bit<F: WriteField<Status<Write>, Value = Interrupt>>() {
        let mut mask = Status::<Write>::default();
        mask.set::<F>(Interrupt::Enabled);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The caller permits interrupt re-entry on this hart.
        unsafe {
            core::arch::asm!("csrs {csr}, {mask}", csr = const Self::NUMBER,
            mask = in(reg) usize::from(mask), options(nostack));
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = mask;
            unimplemented!("CSR access requires RISC-V");
        }
    }
}

impl InterruptEnable {
    /// Atomically enables a supervisor interrupt source.
    ///
    /// # Safety
    ///
    /// Execute on the intended hart with supervisor CSR access and a valid
    /// handler for this source. Permit interrupt delivery without violating
    /// the current exclusion protocol.
    pub unsafe fn set_bit<F: WriteField<InterruptEnable<Write>, Value = Interrupt>>() {
        let mut mask = InterruptEnable::<Write>::default();
        mask.set::<F>(Interrupt::Enabled);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The caller establishes access and permits interrupt delivery.
        unsafe {
            core::arch::asm!("csrs {csr}, {mask}", csr = const Self::NUMBER,
            mask = in(reg) usize::from(mask), options(nostack));
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = mask;
            unimplemented!("CSR access requires RISC-V");
        }
    }

    /// Atomically disables a supervisor interrupt source.
    ///
    /// # Safety
    ///
    /// Execute on the intended hart with supervisor CSR access and coordinate
    /// interrupt delivery.
    pub unsafe fn clear_bit<F: WriteField<InterruptEnable<Write>, Value = Interrupt>>() {
        let mut mask = InterruptEnable::<Write>::default();
        mask.set::<F>(Interrupt::Enabled);
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The caller establishes access and coordinates interrupt delivery.
        unsafe {
            core::arch::asm!("csrc {csr}, {mask}", csr = const Self::NUMBER,
            mask = in(reg) usize::from(mask), options(nostack));
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = mask;
            unimplemented!("CSR access requires RISC-V");
        }
    }
}
