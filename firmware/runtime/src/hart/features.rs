//! Current-hart ISA, privileged-version, and performance-counter discovery.

use super::Xlen;
#[cfg(target_pointer_width = "64")]
use csr::standard::isa::privileged::machine::Mstatus;
use csr::{
    Read,
    standard::isa::{
        privileged::machine::{
            HardwareCounter, Mcounteren, Mcountinhibit, Menvcfg, Misa, measure_stopped_mhpmcounter,
        },
        values::status::Privilege,
    },
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

/// Returns the widths of hardware counters that can both start and stop on this hart.
///
/// A zero width means that the counter could not be measured or controlled.
/// The `time` slot is always zero.
/// Testing the inhibit bits briefly starts stopped counters and stops running ones.
/// A failed HPM width probe can leave a probe value in that counter.
/// Failure to restore an inhibit bit returns [`crate::Error::IoError`].
///
/// # Safety
///
/// Establish the [machine-trap recovery ABI](csr::recovery) with Runtime's
/// direct trap vector installed and `mscratch` zero.
/// Own the current hart's counters and inhibit bits. Coordinate the temporary
/// change to `CY` with all harts sharing the cycle counter.
pub unsafe fn detect_performance_counters() -> crate::Result<[u8; 32]> {
    crate::irq::with_local_disabled(|_| {
        let mut widths = [0; 32];
        // SAFETY: The caller supplies trap recovery for the optional inhibit CSR.
        let Ok(saved) = (unsafe { Mcountinhibit::try_read() }) else {
            return Ok(widths);
        };
        let measure_if_controllable = |counter, measure: fn() -> u8| -> crate::Result<u8> {
            // SAFETY: The caller owns this bit, including a shared CY, and the CSR was readable.
            let set = |stopped| unsafe { crate::pmu::set_inhibited(counter, stopped) };
            let stopped = saved.contains(counter);
            if set(!stopped).is_err() {
                return Ok(0);
            }
            if stopped {
                set(true).map_err(|_| crate::Error::IoError)?;
                Ok(measure())
            } else {
                let width = measure();
                set(false).map_err(|_| crate::Error::IoError)?;
                Ok(width)
            }
        };
        widths[0] = measure_if_controllable(HardwareCounter::Cycle, || 64)?;
        widths[2] = measure_if_controllable(HardwareCounter::InstructionsRetired, || 64)?;
        macro_rules! measure_hpm {
            ($($index:literal),*) => { $( {
                let counter = const { HardwareCounter::new($index).unwrap() };
                // SAFETY: The inhibit bit reads back as set before the measurement.
                widths[$index] = measure_if_controllable(counter, || unsafe {
                    measure_stopped_mhpmcounter::<$index>().unwrap_or(0)
                })?;
            } )* };
        }
        measure_hpm!(
            3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25,
            26, 27, 28, 29, 30, 31
        );
        Ok(widths)
    })
}
