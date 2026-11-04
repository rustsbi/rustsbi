//! CSR probes and current-hart capability discovery.
//!
//! These read-only probes turn recovered illegal instructions into
//! [`Error::IllegalInstruction`](crate::Error::IllegalInstruction).
//! Callers must establish the trap recovery context required by [`Detect`].

use csr::{
    aia::machine::{Mireg, Miselect, Mtopei},
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
    /// Detection reads only the selected register.
    /// An illegal-instruction trap from that read resumes at the generated continuation.
    /// This operation performs no readiness check.
    pub(crate) trait Detect -> crate::Result<()> = crate::Error::IllegalInstruction;

    // Machine information and counter controls used by hart feature discovery.
    CounterEnable,
    CounterInhibit,
    EnvironmentConfig,

    // Debug triggers inspected by DBTR discovery.
    TriggerSelect,
    TriggerData1,

    // AIA indirect access used while initializing a machine IMSIC file.
    Miselect,
    Mireg,
    Mtopei,

    // Architectural counter and supervisor timer compare.
    Time,
    TimerCompare,
    #[cfg(target_pointer_width = "32")]
    TimeHigh,
    #[cfg(target_pointer_width = "32")]
    TimerCompareHigh,

    // Early RV32 trap setup, before the high trap entry is selected.
    #[cfg(target_pointer_width = "32")]
    StatusHigh,
    #[cfg(target_pointer_width = "32")]
    EnvironmentConfigHigh,

    // Optional boot-configuration registers. Runtime first installs its machine
    // trap entry so an absent register can recover.
    SecurityConfig,
    StateEnable0,
    supervisor::StateEnable0,
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
