//! Dynamic firmware-handoff validation and import.

use super::super::{BootInfo, BootInfoError, NextMode, NextStage};
use crate::boot::dtb::copy_from_entry;

pub(crate) const DYNAMIC_MAGIC: usize = 0x4942_534f;

/// The six XLEN-sized words copied from a dynamic firmware handoff.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DynamicWords {
    pub(crate) magic: usize,
    pub(crate) version: usize,
    pub(crate) next_address: usize,
    pub(crate) next_mode: usize,
    pub(crate) options: usize,
    pub(crate) boot_hart: usize,
}

fn validate(words: DynamicWords) -> Result<NextStage, BootInfoError> {
    if words.magic != DYNAMIC_MAGIC {
        return Err(BootInfoError::InvalidBootProtocol);
    }
    match words.version {
        1 | 2 => {}
        _ => return Err(BootInfoError::InvalidBootProtocol),
    }
    if !crate::config::next_address_allowed(words.next_address) {
        return Err(BootInfoError::InvalidBootProtocol);
    }
    let next_mode = match words.next_mode {
        0 => NextMode::User,
        1 => NextMode::Supervisor,
        3 => NextMode::Machine,
        _ => return Err(BootInfoError::InvalidBootProtocol),
    };
    let _ = (words.options, words.boot_hart);
    Ok(NextStage::new(words.next_address, 0, next_mode))
}

/// Constructs complete owned cold-boot input from raw entry envelopes.
///
/// # Safety
///
/// `words` must be the complete fixed snapshot taken by raw entry, and
/// `dtb_address` must satisfy its previous-stage stable readable-memory
/// contract. The caller must own unique cold-boot initialization authority.
pub(crate) unsafe fn prepare_dynamic_boot(
    words: DynamicWords,
    dtb_address: usize,
    init_hart: usize,
) -> Result<BootInfo, BootInfoError> {
    let next_stage = validate(words)?;
    // SAFETY: inherited from this function's DTB-envelope precondition and
    // unique cold-boot authority.
    let dtb = unsafe { copy_from_entry(dtb_address) }?;
    BootInfo::new(dtb, next_stage, init_hart)
}
