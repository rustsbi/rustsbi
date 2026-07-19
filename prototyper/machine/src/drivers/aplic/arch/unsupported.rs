//! Fail-closed backend for non-RISC-V builds outside unit tests.

use super::super::AplicError;

pub(in crate::drivers::aplic) fn configure_device(
    _: usize,
    _: u32,
    _: u64,
    _: u64,
    _: u32,
) -> Result<(), AplicError> {
    Err(AplicError::Readback)
}
