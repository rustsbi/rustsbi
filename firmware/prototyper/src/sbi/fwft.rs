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

fn lock_bit(feature_id: u32) -> Option<u8> {
    match feature_id as usize {
        feature_type::MISALIGNED_EXC_DELEG
        | feature_type::LANDING_PAD
        | feature_type::SHADOW_STACK
        | feature_type::DOUBLE_TRAP
        | feature_type::PTE_AD_HW_UPDATING
        | feature_type::POINTER_MASKING_PMLEN => Some(1u8 << feature_id),
        _ => None,
    }
}

/// Firmware Features extension.
pub(crate) struct SbiFwft {
    locked: PerHart<AtomicU8>,
}

impl SbiFwft {
    /// Creates per-hart FWFT lock state for all enabled harts.
    pub(crate) fn new() -> Result<Self> {
        Ok(Self {
            locked: PerHart::new(|_| AtomicU8::new(0))?,
        })
    }
}

impl Fwft for SbiFwft {
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet {
        let Some(flags) = SetFlags::from_bits(flags) else {
            return SbiRet::invalid_param();
        };
        let Ok(hart) = HartId::current() else {
            return SbiRet::failed();
        };
        let bit = lock_bit(feature_id);
        if let Some(bit) = bit
            && (self.locked.get(hart).load(Ordering::Acquire) & bit) != 0
        {
            return SbiRet::denied_locked();
        }
        match fwft::set(feature_id, value) {
            Ok(()) => {
                if flags.contains(SetFlags::LOCK)
                    && let Some(bit) = bit
                {
                    self.locked.get(hart).fetch_or(bit, Ordering::Release);
                }
                SbiRet::success(0)
            }
            Err(error) => SbiRet::from(error),
        }
    }

    fn get(&self, feature_id: u32) -> SbiRet {
        match fwft::get(feature_id) {
            Ok(value) => SbiRet::success(value),
            Err(error) => SbiRet::from(error),
        }
    }
}
