//! Minimal deterministic backend used only by host unit tests.

use crate::boot::{NextMode, NextStage};
use crate::{CounterError, Trap};

use super::super::{HartTrapState, TrapStateError};

pub(crate) fn current_index() -> Option<usize> {
    Some(0)
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
    panic!("host model cannot park a machine hart")
}

pub(crate) fn enter_resumed_stage(_: NextStage) -> ! {
    panic!("host model cannot resume a machine hart")
}

pub(crate) fn current_state() -> Option<&'static HartTrapState> {
    None
}

pub(crate) fn restore(_: Trap<'_>) -> ! {
    panic!("host model cannot restore a machine trap")
}
