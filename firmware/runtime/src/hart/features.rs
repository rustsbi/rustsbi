//! Current-hart ISA, privileged-version, and performance-counter discovery.

use super::Xlen;
#[cfg(target_pointer_width = "64")]
use csr::standard::isa::privileged::machine::Mstatus;
use csr::{
    Read,
    standard::isa::privileged::machine::{Misa, probe_mhpmcounter},
};
use riscv_csr_spec::standard::isa::{
    privileged::machine::{Mcounteren, Mcountinhibit, Menvcfg},
    values::status::Privilege,
};

/// Returns whether this hart reports the hypervisor extension in `misa`.
pub fn has_hypervisor() -> bool {
    Misa::read().has_extension('H')
}

/// Returns the current hart's effective supervisor XLEN, if S-mode is supported.
///
/// On RV64 this reads `mstatus.SXL`.
/// On RV32 it returns [`Xlen::X32`] whenever `misa.S` is set.
pub fn supervisor_xlen() -> Option<Xlen> {
    if !Misa::read().has_extension('S') {
        return None;
    }
    #[cfg(target_pointer_width = "64")]
    {
        Mstatus::read().get(Mstatus::SXL).ok()
    }
    #[cfg(target_pointer_width = "32")]
    {
        Some(Xlen::X32)
    }
}

/// Checks support for the requested privilege mode.
///
/// M-mode is always supported; S/U support follows the bits reported by `misa`.
pub fn supports_privilege(mode: Privilege) -> bool {
    match mode {
        Privilege::Machine => true,
        Privilege::Supervisor => Misa::read().has_extension('S'),
        Privilege::User => Misa::read().has_extension('U'),
    }
}

/// Compatibility level inferred from readable CSRs on this hart.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum PrivilegedVersion {
    /// No recognized version was detected.
    #[default]
    Unknown = 0,
    /// Machine counter-enable is available.
    Version1_10 = 1,
    /// Machine counter-inhibit is available.
    Version1_11 = 2,
    /// Both machine counter-inhibit and environment configuration are available.
    Version1_12 = 3,
}

/// Classifies this hart by the CSRs usable for counter initialization.
///
/// Returns [`PrivilegedVersion::Unknown`] if recovery is unavailable or neither
/// counter control is readable. Optional CSRs can be absent on newer harts.
pub fn detect_privileged_version() -> PrivilegedVersion {
    if !crate::recovery::is_ready() {
        return PrivilegedVersion::Unknown;
    }
    // SAFETY: The readiness check permits an absent counter-control CSR to trap.
    if unsafe { Mcountinhibit::try_read() }.is_ok() {
        // SAFETY: The same recovery context covers the optional environment CSR.
        if unsafe { Menvcfg::try_read() }.is_ok() {
            PrivilegedVersion::Version1_12
        } else {
            PrivilegedVersion::Version1_11
        }
    } else if unsafe {
        // SAFETY: Recovery from the first probe leaves the trap vector installed.
        Mcounteren::try_read()
    }
    .is_ok()
    {
        PrivilegedVersion::Version1_10
    } else {
        PrivilegedVersion::Unknown
    }
}

/// Returns the width in bits of each hardware counter on this hart.
///
/// Slots 0 through 2 report 64 for cycle, time and instret. Programmable slots
/// report zero if the probe fails or finds no writable bits. If restoration
/// fails, the affected counter or its controls may retain a probe value.
///
/// # Safety
///
/// Run in M-mode with a valid Runtime stack and Runtime trap entry installed.
/// `MPRV` must be clear and `mscratch` must be zero.
/// The caller must exclusively control the current hart's HPM counters, their
/// inhibit bits and event selectors.
pub unsafe fn detect_performance_counters() -> [u8; 32] {
    crate::irq::with_local_disabled(|_| {
        let mut widths = [0; 32];
        widths[..3].fill(64);
        macro_rules! probe {
            ($($index:literal),*) => { $( {
                // SAFETY: The caller owns this hart's counters and establishes recovery.
                widths[$index] = unsafe { probe_mhpmcounter::<$index>() }.unwrap_or(0);
            } )* };
        }
        probe!(
            3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
            26, 27, 28, 29, 30, 31
        );
        widths
    })
}
