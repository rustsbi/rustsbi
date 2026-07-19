//! Fail-closed backend for ordinary non-RISC-V library builds.

use crate::boot::{NextMode, NextStage};
use crate::{CounterError, Trap};

use super::super::{HartTrapState, TrapStateError};

pub(crate) fn current_index() -> Option<usize> {
    None
}

pub(crate) fn activate(_: usize) -> Result<(), TrapStateError> {
    Err(TrapStateError::InvalidIndex)
}

pub(crate) fn prepare_hypervisor_metadata() -> Result<(), TrapStateError> {
    Err(TrapStateError::FeatureProbe)
}

pub(crate) fn prepare_counters(_: usize, _: NextMode) -> Result<(), CounterError> {
    Err(CounterError::MechanismFailure)
}

pub(crate) fn park_current_hart() -> ! {
    panic!("hart parking requires a RISC-V machine target")
}

pub(crate) fn enter_resumed_stage(_: NextStage) -> ! {
    panic!("hart resume requires a RISC-V machine target")
}

pub(crate) fn current_state() -> Option<&'static HartTrapState> {
    None
}

pub(crate) fn restore(_: Trap<'_>) -> ! {
    panic!("trap restore requires a RISC-V machine target")
}
