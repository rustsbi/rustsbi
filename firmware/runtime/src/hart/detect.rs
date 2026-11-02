//! CSR probes and current-hart capability discovery.
//!
//! The declarations use `csr::detect!` to generate the local [`Detect`] trait.
//! Import the trait to call `Register::detect()`.
//! Each generated probe reads the register and turns a recovered illegal instruction into
//! [`Error::IllegalInstruction`](crate::Error::IllegalInstruction).
//! The generated probe does not write the register or check whether trap handling is
//! ready; callers satisfy the method's safety contract.
//!
//! The set is limited to registers already needed by current-hart feature
//! discovery, boot configuration, debug-trigger discovery, IMSIC setup, timer
//! setup, and the early RV32 `mstatush` path.
//! Registers whose reads have side effects are deliberately not declared.

use csr::{
    aia::machine::{Mireg, Miselect},
    hypervisor,
    machine::{
        self, CounterEnable, CounterInhibit, EnvironmentConfig, SecurityConfig, StateEnable0,
        TriggerData1, TriggerSelect,
    },
    supervisor::{self, TimerCompare},
    user::Time,
    vendor::thead::machine::ExtendedStatus,
};
#[cfg(target_pointer_width = "32")]
use csr::{
    machine::{EnvironmentConfigHigh, StatusHigh},
    supervisor::TimerCompareHigh,
    user::TimeHigh,
};

csr::detect! {
    /// Tries to read a selected CSR on the current hart.
    ///
    /// # Safety
    ///
    /// Run on the current hart in M-mode with a valid Runtime stack,
    /// local machine interrupts disabled, `mscratch` and `MPRV` clear, after
    /// Runtime has installed its machine trap entry and `.trap_fixup` recovery.
    /// Detection reads only the selected register. An illegal-instruction
    /// trap from that read resumes at the generated continuation.
    /// This operation performs no readiness check.
    pub(crate) trait Detect -> crate::Result<()> = crate::Error::IllegalInstruction;

    // Machine information and counter controls used by hart feature discovery.
    /// Machine counter-enable register (`mcounteren`, CSR 0x306).
    CounterEnable,
    /// Machine counter-inhibit register (`mcountinhibit`, CSR 0x320).
    CounterInhibit,
    /// Machine environment configuration register (`menvcfg`, CSR 0x30a).
    EnvironmentConfig,

    // Debug triggers inspected by DBTR discovery.
    /// Debug trigger selector (`tselect`, CSR 0x7a0).
    TriggerSelect,
    /// Debug trigger control data (`tdata1`, CSR 0x7a1).
    TriggerData1,

    // AIA indirect access used while initializing a machine IMSIC file.
    /// Machine indirect register selector (`miselect`, CSR 0x350).
    Miselect,
    /// Machine indirect register data (`mireg`, CSR 0x351).
    Mireg,

    // Architectural counter and supervisor timer compare.
    /// Architectural time counter (`time`, CSR 0xc01).
    Time,
    /// Supervisor timer compare (`stimecmp`, CSR 0x14d).
    TimerCompare,
    /// Upper half of the time counter (`timeh`, RV32 only).
    #[cfg(target_pointer_width = "32")]
    TimeHigh,
    /// Upper half of supervisor timer compare (`stimecmph`, RV32 only).
    #[cfg(target_pointer_width = "32")]
    TimerCompareHigh,

    // Early RV32 trap setup, before the high trap entry is selected.
    /// Upper machine status register (`mstatush`, CSR 0x310).
    ///
    /// This register exists only on RV32.
    #[cfg(target_pointer_width = "32")]
    StatusHigh,
    /// Upper half of `menvcfg` (`menvcfgh`, RV32 only).
    #[cfg(target_pointer_width = "32")]
    EnvironmentConfigHigh,

    // Optional boot-configuration registers. Runtime first installs its machine
    // trap entry so an absent register can recover.
    /// Machine security configuration register (`mseccfg`, CSR 0x747).
    SecurityConfig,
    /// Machine state-enable register zero (`mstateen0`, CSR 0x30c).
    StateEnable0,
    /// Supervisor state-enable register zero (`sstateen0`, CSR 0x10c).
    supervisor::StateEnable0,
    /// Hypervisor state-enable register zero (`hstateen0`, CSR 0x60c).
    hypervisor::StateEnable0,
    /// T-Head extended status register (`mxstatus`, CSR 0x7c0).
    ///
    /// See the Xuantie OpenC910 hardware source:
    /// <https://github.com/XUANTIE-RV/openc910/blob/main/C910_RTL_FACTORY/gen_rtl/cp0/rtl/ct_cp0_iui.v#L615>.
    ExtendedStatus,
}

// Programmable machine performance counters 3 through 31.
seq_macro::seq!(N in 3..=31 {
    csr::detect! {
        impl Detect -> crate::Result<()> = crate::Error::IllegalInstruction;
        #(machine::PerformanceCounter<N>,)*
    }
});
