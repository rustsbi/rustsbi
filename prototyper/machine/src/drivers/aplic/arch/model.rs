//! Side-effect-free APLIC transport used by binding integration tests.

use super::super::AplicError;

pub(in crate::drivers::aplic) fn configure_device(
    _: usize,
    _: u32,
    _: u64,
    _: u64,
    _: u32,
) -> Result<(), AplicError> {
    Ok(())
}
