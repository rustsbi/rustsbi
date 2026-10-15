//! Current-hart ISA, privileged-version, and performance-counter discovery.

use crate::csr::instruction;
use csr::machine;

/// Returns whether this hart reports the hypervisor extension in `misa`.
pub fn has_hypervisor() -> bool {
    machine::Isa::read().has_hypervisor()
}

/// Returns whether this hart reports the requested privilege mode in `misa`.
pub fn supports_privilege(mode: riscv::register::mstatus::MPP) -> bool {
    use riscv::register::mstatus::MPP;
    match mode {
        MPP::Machine => true,
        MPP::Supervisor => machine::Isa::read().has_supervisor(),
        MPP::User => machine::Isa::read().has_user(),
    }
}

/// Privileged-architecture version inferred from implemented standard CSRs.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrivilegedVersion {
    /// No recognized version was detected.
    #[default]
    Unknown = 0,
    /// Machine counter-enable is available.
    Version1_10 = 1,
    /// Machine counter-inhibit is also available.
    Version1_11 = 2,
    /// Machine environment configuration is also available.
    Version1_12 = 3,
}

/// Detects the current hart's version after Runtime trap installation.
pub fn detect_privileged_version() -> PrivilegedVersion {
    if !crate::boot::traps_ready() {
        return PrivilegedVersion::Unknown;
    }
    if instruction!("csrr {value}, {csr}", machine::CounterEnable::NUMBER, 0).is_err() {
        PrivilegedVersion::Unknown
    } else if instruction!("csrr {value}, {csr}", machine::CounterInhibit::NUMBER, 0).is_err() {
        PrivilegedVersion::Version1_10
    } else if instruction!(
        "csrr {value}, {csr}",
        machine::EnvironmentConfiguration::NUMBER,
        0
    )
    .is_err()
    {
        PrivilegedVersion::Version1_11
    } else {
        PrivilegedVersion::Version1_12
    }
}

/// Discovers writable performance counters during current-hart startup.
///
/// # Safety
///
/// Run after Runtime entry installation, before publishing the PMU.
/// Own the current hart's counters with MPRV clear and mscratch zero.
pub unsafe fn detect_performance_counters() -> u32 {
    crate::irq::with_local_disabled(|_| {
        // Preserve the existing convention for cycle, time, and instret.
        let mut mask = 0b111;
        macro_rules! probe {
            ($($index:literal),*) => { $( {
                const NUMBER: u16 = machine::PerformanceCounter::<$index>::NUMBER;
                if let Ok(old) = instruction!("csrr {value}, {csr}", NUMBER, 0)
                    && instruction!("csrw {csr}, {value}", NUMBER, 1).is_ok()
                    && instruction!("csrrw {value}, {csr}, {value}", NUMBER, old) == Ok(1) {
                    mask |= 1 << $index;
                }
            } )* };
        }
        probe!(
            3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
            26, 27, 28, 29, 30, 31
        );
        mask
    })
}
