//! K1 PMU wakeup and reset-vector register protocols.

mod hart_wake;
mod reset_vector;

pub(crate) use hart_wake::K1HartWake;
pub(crate) use reset_vector::ResetVectorRegisters;
