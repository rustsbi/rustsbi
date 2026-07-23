//! Build-selected Supervisor next-stage import.

use super::super::{BootInfo, BootInfoError, NextMode, NextStage};
use crate::boot::dtb::copy_from_entry;

/// Constructs owned cold-boot input for a build-selected Supervisor stage.
///
/// # Safety
///
/// `dtb_address` must satisfy the previous-stage stable readable-memory
/// contract, and the caller must own unique cold-boot initialization authority.
pub(crate) unsafe fn prepare_fixed_boot(
    next_address: usize,
    next_mode: NextMode,
    dtb_address: usize,
    init_hart: usize,
) -> Result<BootInfo, BootInfoError> {
    if !crate::config::next_address_allowed(next_address) {
        return Err(BootInfoError::InvalidBootProtocol);
    }
    let next_stage = NextStage::new(next_address, 0, next_mode);
    // SAFETY: inherited from this function's DTB-envelope precondition and
    // unique cold-boot authority.
    let dtb = unsafe { copy_from_entry(dtb_address) }?;
    BootInfo::new(dtb, next_stage, init_hart)
}
