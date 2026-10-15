//! Virtual supervisor registers.
use super::machine::{Interrupt, SupervisorPrivilege};
use crate::macros::csr;
use crate::values::{
    Address, TrapValueBits,
    trap::CauseValue,
    vector::{TargetVectorMode, VectorBase, VectorMode},
};

csr! {
    /// Virtual supervisor status register (`vsstatus`).
    ///
    /// See
    /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#vsstatus>.
    Status { 0x200, r,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#vsstatus>.
        ///
        /// # Safety
        ///
        /// Preserve the guest execution state used by the next virtual `sret`.
        unsafe w
    }
        SIE[1] { rw: Interrupt }
        SPIE[5] { rw: Interrupt }
        SPP[8] { rw: SupervisorPrivilege }
    /// Virtual supervisor trap-vector register (`vstvec`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#vstvec>.
    TrapVector { 0x205, r,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#vstvec>.
        ///
        /// # Safety
        ///
        /// The vector and guest trap stack must preserve the guest execution
        /// context.
        unsafe w
    }
        MODE[0..=1] { r: VectorMode, w: TargetVectorMode }
        #[cfg(target_pointer_width = "64")]
        BASE[2..=63] { rw: VectorBase }
        #[cfg(target_pointer_width = "32")]
        BASE[2..=31] { rw: VectorBase }
    /// Virtual supervisor exception program counter (`vsepc`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#vsepc>.
    ExceptionPc { 0x241, r: Address,
        /// See
        /// <https://docs.riscv.org/reference/isa/priv/hypervisor.html#vsepc>.
        ///
        /// # Safety
        ///
        /// The next virtual `sret` must resume a valid guest execution context.
        unsafe w: Address
    }
    /// Virtual supervisor trap cause register (`vscause`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#vscause>.
    Cause { 0x242, r: CauseValue, w: CauseValue }
    /// Virtual supervisor trap value register (`vstval`).
    ///
    /// See <https://docs.riscv.org/reference/isa/priv/hypervisor.html#vstval>.
    TrapValue { 0x243, r: TrapValueBits, w: TrapValueBits }
}

impl TrapVectorWriteValue {
    /// Combines a base address and dispatch mode; hardware may require stricter alignment.
    pub fn new(base: VectorBase, mode: TargetVectorMode) -> Self {
        let mut value = Self(0);
        value.write::<BASE>(base);
        value.write::<MODE>(mode);
        value
    }
}

impl StatusWriteValue {
    /// Creates guest trap-entry state with interrupts and optional state
    /// disabled.
    pub fn for_trap(previous: SupervisorPrivilege, interrupts: Interrupt) -> Self {
        let mut value = Self(0);
        value.write::<SPP>(previous);
        value.write::<SPIE>(interrupts);
        value
    }
}
