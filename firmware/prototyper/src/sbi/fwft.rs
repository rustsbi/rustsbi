//! SBI FWFT argument and result adaptation.

#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicU8, Ordering};

use runtime::{
    Result, fwft,
    hart::{HartId, PerHart},
    rustsbi::{
        Fwft, SbiRet,
        spec::fwft::{feature_type, flags::SetFlags},
    },
};

const LOCAL_FEATURES: usize = feature_type::POINTER_MASKING_PMLEN + 1;
// Locked values are Boolean or a pointer mask length in bits (at most XLEN).
const UNLOCKED: u8 = u8::MAX;

/// Firmware Features extension.
pub(crate) struct SbiFwft {
    locked: PerHart<[AtomicU8; LOCAL_FEATURES]>,
}

impl SbiFwft {
    /// Creates per-hart FWFT lock state for all enabled harts.
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            locked: PerHart::new(|_| [const { AtomicU8::new(UNLOCKED) }; LOCAL_FEATURES])?,
        })
    }

    fn lock_slot(&self, feature_id: u32) -> Result<&AtomicU8, SbiRet> {
        let hart = HartId::current().map_err(|_| SbiRet::failed())?;
        if feature_id as usize == feature_type::PTE_AD_HW_UPDATING {
            let extensions = super::features::supervisor_envcfg(hart);
            if !(extensions.svade && extensions.svadu) {
                return Err(SbiRet::not_supported());
            }
        }
        self.locked
            .get(hart)
            .get(feature_id as usize)
            .ok_or_else(SbiRet::denied)
    }
}

/// Drops the local FWFT locks when a secondary hart enters firmware after a hardware reset.
pub(crate) fn reset_hart(hart: HartId) {
    if let Some(fwft) = super::SbiDispatcher::get().and_then(|sbi| sbi.fwft.as_ref()) {
        for slot in fwft.locked.get(hart) {
            slot.store(UNLOCKED, Ordering::Release);
        }
    }
}

impl Fwft for SbiFwft {
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet {
        let Some(flags) = SetFlags::from_bits(flags) else {
            return SbiRet::invalid_param();
        };
        let slot = match self.lock_slot(feature_id) {
            Ok(slot) => slot,
            Err(error) => return error,
        };
        let locked = slot.load(Ordering::Acquire);
        if locked != UNLOCKED {
            return if usize::from(locked) == value {
                SbiRet::success(0)
            } else {
                SbiRet::denied_locked()
            };
        }
        match fwft::set(feature_id, value) {
            Ok(()) => {
                if flags.contains(SetFlags::LOCK) {
                    slot.store(value as u8, Ordering::Release);
                }
                SbiRet::success(0)
            }
            Err(error) => SbiRet::from(error),
        }
    }

    fn get(&self, feature_id: u32) -> SbiRet {
        let slot = match self.lock_slot(feature_id) {
            Ok(slot) => slot,
            Err(error) => return error,
        };
        let locked = slot.load(Ordering::Acquire);
        if locked != UNLOCKED {
            return SbiRet::success(usize::from(locked));
        }
        match fwft::get(feature_id) {
            Ok(value) => SbiRet::success(value),
            Err(error) => SbiRet::from(error),
        }
    }
}
