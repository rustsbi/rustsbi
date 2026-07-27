//! RISC-V privileged counter access and lower-mode enable policy.

use crate::boot::NextMode;
use crate::trap::probe::{ExpectedResult, probe_csr, swap_csr};

use super::{CounterError, PerformanceCounters};

const MCOUNTEREN: u16 = 0x306;
const SCOUNTEREN: u16 = 0x106;
const TIME_COUNTER: usize = 1 << 1;

/// Installs the RISC-V `mcounteren`/`scounteren` policy for the current hart.
///
/// The counter masks come only from the current hart's successfully probed
/// Zicntr/Zihpm state. Time remains readable in S-mode so the trap path can
/// emulate unavailable direct reads without granting arbitrary CSR access.
pub(super) fn prepare_counter_access(
    mode: NextMode,
    counters: &PerformanceCounters,
) -> Result<(), CounterError> {
    let accessible = counters.accessible_mask()? as usize;
    let (machine_access, supervisor_access) = access_masks(mode, accessible);
    let actual_machine = write_checked::<MCOUNTEREN>(machine_access, mode)?;
    write_checked::<SCOUNTEREN>(supervisor_access, mode)?;
    counters.retain_current((actual_machine & accessible) as u32)
}

const fn access_masks(mode: NextMode, accessible: usize) -> (usize, usize) {
    match mode {
        NextMode::Supervisor => (accessible | TIME_COUNTER, TIME_COUNTER),
        NextMode::User => (TIME_COUNTER, TIME_COUNTER),
        NextMode::Machine => (0, 0),
    }
}

fn write_checked<const CSR: u16>(value: usize, mode: NextMode) -> Result<usize, CounterError> {
    // SAFETY: callers instantiate only the fixed RISC-V counter-enable CSRs.
    match unsafe { swap_csr::<CSR>(value) } {
        ExpectedResult::Value(_) => match unsafe { probe_csr::<CSR>() } {
            ExpectedResult::Value(actual) if actual == value => Ok(actual),
            ExpectedResult::Fault(fault)
                if fault.cause == 2 && mode == NextMode::Machine && value == 0 =>
            {
                Ok(0)
            }
            _ => Err(CounterError::MechanismFailure),
        },
        ExpectedResult::Fault(fault)
            if fault.cause == 2 && mode == NextMode::Machine && value == 0 =>
        {
            Ok(0)
        }
        _ => Err(CounterError::MechanismFailure),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_masks_are_closed_by_next_mode() {
        let implemented = (1 << 0) | (1 << 2) | (1 << 7);
        assert_eq!(access_masks(NextMode::Supervisor, implemented), (0x87, 0x2));
        assert_eq!(access_masks(NextMode::User, implemented), (0x2, 0x2));
        assert_eq!(access_masks(NextMode::Machine, implemented), (0, 0));
    }
}
