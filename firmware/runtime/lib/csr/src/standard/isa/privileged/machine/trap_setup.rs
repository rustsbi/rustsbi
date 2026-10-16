//! Machine trap setup.
use crate::macros::csr;
use crate::values::{
    counter::CounterSet,
    trap::{ExceptionSet, InterruptSet},
    vector::{TargetVectorMode, VectorBase, VectorMode},
};

csr! {
    pub struct IsaValue(usize);

    /// Machine privilege, interrupt, and extension state (`mstatus`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mstatus>
    Status { 0x300, r,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mstatus>.
        ///
        /// # Safety
        ///
        /// Preserve address-space, extension-state, and interrupt-exclusion invariants.
        /// `MPRV` selects the effective privilege of subsequent Rust loads and stores.
        unsafe w
    }
        SIE[1] { rw: InterruptState { Disabled = 0, Enabled = 1 } }
        MIE[3] { rw: InterruptState }
        SPIE[5] { rw: InterruptState }
        UBE[6] { rw: Endianness { Little = 0, Big = 1 } }
        MPIE[7] { rw: InterruptState }
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
        GVA[38] { rw: GuestAddress { Other = 0, GuestVirtual = 1 } }
        #[cfg(target_pointer_width = "64")]
        MPV[39] { rw: Virtualization { Off = 0, On = 1 } }
        #[cfg(target_pointer_width = "64")]
        MDT[42] { rw: DoubleTrapState { Clear = 0, Set = 1 } }

    /// Supported ISA extensions and base integer width (`misa`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#misa>
    Isa { 0x301, r: IsaValue,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#misa>.
        ///
        /// # Safety
        ///
        /// The selected extensions and instruction alignment must support all live
        /// code, register state, and interrupt handlers on this hart.
        unsafe w: IsaValue
    }
    /// Exceptions delegated to supervisor mode (`medeleg`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#medeleg>
    ExceptionDelegation { 0x302, r: ExceptionSet, w: ExceptionSet }
    /// Interrupts delegated to supervisor mode (`mideleg`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mideleg>
    InterruptDelegation { 0x303, r: InterruptSet, w: InterruptSet }
    /// Machine interrupt-source enables (`mie`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mie>
    InterruptEnable { 0x304, r,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mie>.
        ///
        /// # Safety
        ///
        /// Install valid handlers for enabled sources.
        /// Permit their re-entry without violating active exclusion or memory invariants.
        unsafe w
    }
        MSIE[3] { rw: InterruptState }
        MTIE[7] { rw: InterruptState }
        MEIE[11] { rw: InterruptState }
    /// Machine trap entry address and dispatch mode (`mtvec`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mtvec>
    TrapVector { 0x305, r,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mtvec>.
        ///
        /// # Safety
        ///
        /// The vector must preserve Rust execution state, with a valid trap stack.
        unsafe w
    }
        MODE[0..=1] { r: VectorMode, w: TargetVectorMode }
        #[cfg(target_pointer_width = "64")]
        BASE[2..=63] { rw: VectorBase }
        #[cfg(target_pointer_width = "32")]
        BASE[2..=31] { rw: VectorBase }
    /// Counter access permitted to lower privilege modes (`mcounteren`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mcounteren>
    CounterEnable { 0x306, r: CounterSet, w: CounterSet }
    /// Upper machine status bits on RV32 (`mstatush`).
    ///
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mstatush>
    #[cfg(target_pointer_width = "32")]
    StatusHigh { 0x310, r,
        /// See <https://docs.riscv.org/reference/isa/priv/machine.html#mstatush>.
        ///
        /// # Safety
        ///
        /// Preserve address translation, endianness, and trap exclusion on this hart.
        unsafe w
    }
        SBE[4] { rw: Endianness }
        GVA[6] { rw: GuestAddress { Other = 0, GuestVirtual = 1 } }
        MPV[7] { rw: Virtualization { Off = 0, On = 1 } }
        MDT[10] { rw: DoubleTrapState { Clear = 0, Set = 1 } }
}

impl InterruptEnableWriteValue {
    /// Disables all interrupt sources in the complete write value.
    pub const fn disabled() -> Self {
        Self(0)
    }
}

impl From<bool> for Virtualization {
    fn from(virtualized: bool) -> Self {
        if virtualized { Self::On } else { Self::Off }
    }
}

impl StatusWriteValue {
    /// Creates a return context with interrupts and optional state disabled.
    pub const fn for_return(privilege: TargetPrivilege) -> Self {
        Self((privilege as usize) << MPP::MASK.trailing_zeros())
    }
}

impl IsaValue {
    pub const fn has_hypervisor(self) -> bool {
        self.0 & (1 << 7) != 0
    }
}

#[cfg(target_pointer_width = "32")]
impl StatusHighWriteValue {
    /// Creates a nonvirtual return context with big-endian accesses disabled.
    pub const fn native() -> Self {
        Self(0)
    }
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
