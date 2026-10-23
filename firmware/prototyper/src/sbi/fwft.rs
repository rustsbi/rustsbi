//! SBI firmware-feature policy over Runtime's hardware configuration.

#![forbid(unsafe_code)]

use runtime::{
    Error, Result,
    fwft::{Feature, Fwft, PointerMasking, Value},
    hart::{HartId, HartLocal},
    rustsbi::{self, SbiRet},
};
use rustsbi::spec::fwft::{feature_type, flags::SetFlags};
use spin::Mutex;

/// SBI Firmware Features extension.
pub(crate) struct SbiFwft {
    fwft: Fwft,
    locks: HartLocal<Mutex<[bool; Feature::COUNT]>>,
}

impl SbiFwft {
    pub(crate) fn new(fwft: Fwft) -> Result<Self> {
        let locks = HartLocal::new();
        locks.init_all(|_| Mutex::new([false; Feature::COUNT]))?;
        Ok(Self { fwft, locks })
    }
    pub(crate) fn fwft(&self) -> &Fwft {
        &self.fwft
    }
}

impl rustsbi::Fwft for SbiFwft {
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet {
        let result: Result<usize> = (|| {
            let feature = Feature::try_from(FeatureId(feature_id))?;
            let flags = SetFlags::from_bits(flags).ok_or(Error::InvalidArgs)?;
            let value = Value::try_from(FeatureValue {
                feature,
                raw: value,
            })?;
            self.fwft.with_current(|features| {
                let mut locks = self
                    .locks
                    .get(HartId::current()?)?
                    .try_lock()
                    .ok_or(Error::Busy)?;
                let locked = &mut locks[feature as usize];
                if *locked {
                    return Err(Error::Locked);
                }
                features.set(feature, value)?;
                *locked = flags.contains(SetFlags::LOCK);
                Ok(())
            })?;
            Ok(0)
        })();
        result.map_or_else(Into::into, SbiRet::success)
    }

    fn get(&self, feature_id: u32) -> SbiRet {
        let result: Result<usize> = (|| {
            let feature = Feature::try_from(FeatureId(feature_id))?;
            let value = self.fwft.with_current(|features| features.get(feature))?;
            Ok(match value {
                Value::Enabled(enabled) => usize::from(enabled),
                Value::PointerMasking(mode) => mode.pmlen(),
            })
        })();
        result.map_or_else(Into::into, SbiRet::success)
    }
}

struct FeatureId(u32);

impl TryFrom<FeatureId> for Feature {
    type Error = Error;

    fn try_from(id: FeatureId) -> Result<Self> {
        Ok(match id.0 as usize {
            feature_type::MISALIGNED_EXC_DELEG => Feature::MisalignedDelegation,
            feature_type::LANDING_PAD => Feature::LandingPad,
            feature_type::SHADOW_STACK => Feature::ShadowStack,
            feature_type::DOUBLE_TRAP => Feature::DoubleTrap,
            feature_type::PTE_AD_HW_UPDATING => Feature::PteAdUpdating,
            feature_type::POINTER_MASKING_PMLEN => Feature::PointerMasking,
            _ => return Err(Error::AccessDenied),
        })
    }
}

struct FeatureValue {
    feature: Feature,
    raw: usize,
}

impl TryFrom<FeatureValue> for Value {
    type Error = Error;

    fn try_from(value: FeatureValue) -> Result<Self> {
        if value.feature == Feature::PointerMasking {
            PointerMasking::from_pmlen(value.raw)
                .map(Value::PointerMasking)
                .ok_or(Error::InvalidArgs)
        } else {
            match value.raw {
                0 => Ok(Value::Enabled(false)),
                1 => Ok(Value::Enabled(true)),
                _ => Err(Error::InvalidArgs),
            }
        }
    }
}
