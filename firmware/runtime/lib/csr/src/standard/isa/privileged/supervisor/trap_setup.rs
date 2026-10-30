//! Supervisor trap setup.
use super::super::machine::{Interrupt, SupervisorPrivilege};
use crate::macros::csr;
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
    InterruptEnable { 0x104, r: InterruptSet,
        /// See <https://docs.riscv.org/reference/isa/priv/supervisor.html#sie>.
        ///
        /// # Safety
        ///
        /// Enabled interrupt handlers must preserve the interrupted execution
        /// state.
        unsafe w: InterruptSet
    }
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
