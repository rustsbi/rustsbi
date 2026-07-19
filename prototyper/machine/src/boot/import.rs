//! RISC-V cold-entry import into owned boot state.

use super::BootInfo;
use super::dtb::copy_from_entry;
use super::protocol::{BootInfoError, NextStage};
#[cfg(any(feature = "jump", feature = "payload"))]
use super::protocol::{DynamicBootError, NextMode};
#[cfg(not(any(feature = "jump", feature = "payload")))]
use super::protocol::{DynamicWords, validate_dynamic};

/// Constructs complete owned cold-boot input from raw entry envelopes.
///
/// # Safety
///
/// `words` must be the complete fixed snapshot taken by raw entry, and
/// `dtb_address` must satisfy its previous-stage stable readable-memory
/// contract. The caller must own unique cold-boot initialization authority.
#[cfg(not(any(feature = "jump", feature = "payload")))]
pub(crate) unsafe fn prepare_dynamic_boot(
    words: DynamicWords,
    dtb_address: usize,
    init_hart: usize,
) -> Result<BootInfo, BootInfoError> {
    let dynamic = validate_dynamic(words, crate::config::next_address_allowed)?;
    // SAFETY: inherited from this function's DTB-envelope precondition and
    // unique cold-boot authority.
    let dtb = unsafe { copy_from_entry(dtb_address) }?;
    BootInfo::new(dtb, NextStage::from_dynamic(dynamic), init_hart)
}

/// Constructs owned cold-boot input for a configured Supervisor payload.
///
/// # Safety
///
/// `dtb_address` must satisfy the previous-stage stable readable-memory
/// contract, and the caller must own unique cold-boot initialization authority.
#[cfg(any(feature = "jump", feature = "payload"))]
pub(crate) unsafe fn prepare_fixed_boot(
    next_address: usize,
    dtb_address: usize,
    init_hart: usize,
) -> Result<BootInfo, BootInfoError> {
    if !crate::config::next_address_allowed(next_address) {
        return Err(DynamicBootError::InvalidNextAddress.into());
    }
    let next_stage = NextStage {
        entry: next_address,
        opaque: 0,
        mode: NextMode::Supervisor,
    };
    // SAFETY: inherited from this function's DTB-envelope precondition and
    // unique cold-boot authority.
    let dtb = unsafe { copy_from_entry(dtb_address) }?;
    BootInfo::new(dtb, next_stage, init_hart)
}
