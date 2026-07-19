//! RISC-V machine-CSR probes and policy installation.

use super::{PrepareError, ProbeError, TIME_COUNTER, misa_has_hypervisor};
use crate::boot::NextMode;
use crate::counter::{CounterError, PerformanceCounters};
use crate::trap::expected::{ExpectedResult, probe_csr, swap_csr};

const MCOUNTEREN: u16 = 0x306;
const MISA: u16 = 0x301;
const MTINST: u16 = 0x34a;
const MTVAL2: u16 = 0x34b;
const SCOUNTEREN: u16 = 0x106;
const HSTATUS: u16 = 0x600;

fn probe<const CSR: u16>() -> Result<Option<usize>, ProbeError> {
    // SAFETY: instantiations below name only fixed responsibility-specific CSRs.
    match unsafe { probe_csr::<CSR>() } {
        ExpectedResult::Value(value) => Ok(Some(value)),
        ExpectedResult::Fault(fault) if fault.cause == 2 => Ok(None),
        ExpectedResult::Fault(_) => Err(ProbeError::UnexpectedFault),
        ExpectedResult::Busy => Err(ProbeError::Busy),
        ExpectedResult::Unavailable => Err(ProbeError::RuntimeUnavailable),
    }
}

/// Detects complete current-hart H trap-metadata support.
pub(crate) fn probe_hypervisor_metadata() -> Result<bool, ProbeError> {
    let Some(misa) = probe::<MISA>()? else {
        return Ok(false);
    };
    if !misa_has_hypervisor(misa) {
        return Ok(false);
    }
    for available in [
        probe::<MTINST>()?.is_some(),
        probe::<MTVAL2>()?.is_some(),
        probe::<HSTATUS>()?.is_some(),
    ] {
        if !available {
            return Err(ProbeError::UnexpectedFault);
        }
    }
    Ok(true)
}

#[crate::mtest]
fn machine_identity_matches_xlen() {
    let expected_mxl = if usize::BITS == 32 { 1 } else { 2 };
    let misa = probe::<MISA>()
        .expect("misa probe must complete")
        .expect("misa must be implemented");
    assert_eq!(misa >> (usize::BITS - 2), expected_mxl);
}

/// Installs the closed lower-privilege counter-access policy for one hart.
pub(crate) fn prepare_counter_access(
    mode: NextMode,
    counters: &PerformanceCounters,
) -> Result<(), PrepareError> {
    let accessible = counters
        .accessible_mask()
        .map_err(|_: CounterError| PrepareError::Counter)? as usize;
    let (machine_access, supervisor_access) = match mode {
        NextMode::Supervisor => (accessible | TIME_COUNTER, TIME_COUNTER),
        NextMode::User => (TIME_COUNTER, TIME_COUNTER),
        NextMode::Machine => (0, 0),
    };
    let actual_machine = write_checked::<MCOUNTEREN>(machine_access, mode)?;
    write_checked::<SCOUNTEREN>(supervisor_access, mode)?;
    counters
        .retain_current((actual_machine & accessible) as u32)
        .map_err(|_: CounterError| PrepareError::Counter)?;
    Ok(())
}

fn write_checked<const CSR: u16>(value: usize, mode: NextMode) -> Result<usize, PrepareError> {
    // SAFETY: callers instantiate only the two fixed counter-access CSRs.
    let result = unsafe { swap_csr::<CSR>(value) };
    match result {
        ExpectedResult::Value(_) => match probe::<CSR>() {
            Ok(Some(actual)) if actual == value => Ok(actual),
            Ok(Some(_)) => Err(PrepareError::Readback),
            Ok(None) if mode == NextMode::Machine && value == 0 => Ok(0),
            Ok(None) => Err(PrepareError::Unavailable),
            Err(ProbeError::UnexpectedFault) => Err(PrepareError::UnexpectedFault),
            Err(ProbeError::Busy | ProbeError::RuntimeUnavailable) => {
                Err(PrepareError::Unavailable)
            }
        },
        ExpectedResult::Fault(fault)
            if fault.cause == 2 && mode == NextMode::Machine && value == 0 =>
        {
            Ok(0)
        }
        ExpectedResult::Fault(fault) if fault.cause == 2 => Err(PrepareError::Unavailable),
        ExpectedResult::Fault(_) => Err(PrepareError::UnexpectedFault),
        ExpectedResult::Busy | ExpectedResult::Unavailable => Err(PrepareError::Unavailable),
    }
}
