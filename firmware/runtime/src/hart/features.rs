//! Current-hart ISA, privileged-version, and performance-counter discovery.

use crate::csr::{csrr, csrrw, csrw};
use csr::machine;

/// Returns whether this hart reports the hypervisor extension in `misa`.
pub fn has_hypervisor() -> bool {
    machine::Isa::read().has_extension('H')
}

/// Checks support for the requested privilege mode.
///
/// M-mode is always supported; S/U support follows the bits reported by `misa`.
pub fn supports_privilege(mode: machine::TargetPrivilege) -> bool {
    match mode {
        machine::TargetPrivilege::Machine => true,
        machine::TargetPrivilege::Supervisor => machine::Isa::read().has_extension('S'),
        machine::TargetPrivilege::User => machine::Isa::read().has_extension('U'),
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
///
/// Returns [`PrivilegedVersion::Unknown`] before the trap entry is installed.
pub fn detect_privileged_version() -> PrivilegedVersion {
    if !crate::boot::traps_ready() {
        return PrivilegedVersion::Unknown;
    }
    if csrr!(machine::CounterEnable).is_err() {
        PrivilegedVersion::Unknown
    } else if csrr!(machine::CounterInhibit).is_err() {
        PrivilegedVersion::Version1_10
    } else if csrr!(machine::EnvironmentConfig).is_err() {
        PrivilegedVersion::Version1_11
    } else {
        PrivilegedVersion::Version1_12
    }
}

/// Discovers writable performance counters during current-hart startup.
///
/// # Safety
///
/// Run in M-mode with a valid Runtime stack and Runtime trap entry installed.
/// `MPRV` must be clear and `mscratch` must be zero.
/// The caller must exclusively control the current hart's hardware counters
/// and permit the probe to modify them.
pub unsafe fn detect_performance_counters() -> u32 {
    crate::irq::with_local_disabled(|_| {
        // Preserve the existing convention for cycle, time, and instret.
        let mut mask = 0b111;
        macro_rules! probe {
            ($($index:literal),*) => { $( {
                if let Ok(old) = csrr!(machine::PerformanceCounter<$index>)
                    && csrw!(machine::PerformanceCounter<$index>, 1).is_ok()
                    && csrrw!(machine::PerformanceCounter<$index>, old) == Ok(1) {
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
