//! SBI firmware-feature policy over Runtime's hardware configuration.

#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicBool, Ordering};
use runtime::{
    Error, Result,
    fwfeatures::{
        Feature as HardwareFeature, FeatureState as HardwareFeatureState, Features, FwFeatures,
        PointerMasking, Value,
    },
    hart::{HartId, HartLocal},
    rustsbi::spec::binary::Error as SbiError,
    rustsbi::{self, Configuration, Feature, Lock, spec::fwft::FeatureType},
};

/// Number of hardware features exposed through FWFT.
const HARDWARE_FEATURE_COUNT: usize = 6;

/// SBI Firmware Features extension.
pub struct SbiFwft {
    locked_features: HartLocal<FeatureLocks>,
}

impl SbiFwft {
    pub fn new() -> Result<Self> {
        let locked_features = HartLocal::new();
        locked_features.init_all(|_| FeatureLocks::new())?;
        Ok(Self { locked_features })
    }

    fn apply(
        &self,
        features: &mut Features<'_>,
        feature: HardwareFeature,
        value: Value,
        lock: Lock,
    ) -> Result<()> {
        let locks = self.locked_features.get(HartId::current()?)?;
        if locks.is_locked(feature) && features.get(feature)? != value {
            return Err(Error::Locked);
        }
        features.set(feature, value)?;
        if lock == Lock::Lock {
            locks.lock(feature);
        }
        Ok(())
    }
}

impl rustsbi::Fwft for SbiFwft {
    fn set(&self, configuration: Configuration, lock: Lock) -> core::result::Result<(), SbiError> {
        let (feature, value) = hardware_configuration(configuration)?;
        FwFeatures::with_current(|features| self.apply(features, feature, value, lock))
            .map_err(sbi_error)
    }

    fn get(&self, feature: Feature) -> core::result::Result<Configuration, SbiError> {
        let hardware = hardware_feature(feature)?;
        let value =
            FwFeatures::with_current(|features| features.get(hardware)).map_err(sbi_error)?;
        sbi_configuration(feature, value)
    }
}

/// Accessed only while Runtime holds the current hart's feature transaction.
/// Atomics provide interior mutability; the transaction orders the lock check,
/// hardware update, and lock commit together.
struct FeatureLocks {
    locked: [AtomicBool; HARDWARE_FEATURE_COUNT],
}

impl FeatureLocks {
    fn new() -> Self {
        Self {
            locked: [const { AtomicBool::new(false) }; HARDWARE_FEATURE_COUNT],
        }
    }

    fn is_locked(&self, feature: HardwareFeature) -> bool {
        self.locked[feature.id() as usize].load(Ordering::Relaxed)
    }

    fn lock(&self, feature: HardwareFeature) {
        self.locked[feature.id() as usize].store(true, Ordering::Relaxed);
    }
}

fn hardware_feature(feature: Feature) -> core::result::Result<HardwareFeature, SbiError> {
    Ok(match feature {
        Feature::Standard(feature) => feature,
        Feature::PlatformSpecific(_) => return Err(SbiError::Denied),
    })
}

fn hardware_configuration(
    configuration: Configuration,
) -> core::result::Result<(HardwareFeature, Value), SbiError> {
    let feature = hardware_feature(configuration.feature())?;
    let value = if configuration.feature().is_boolean() {
        Value::State(match configuration.value() {
            0 => HardwareFeatureState::Disabled,
            1 => HardwareFeatureState::Enabled,
            _ => return Err(SbiError::InvalidParam),
        })
    } else if configuration.feature() == Feature::Standard(FeatureType::PointerMaskingPmlen) {
        Value::PointerMasking(
            PointerMasking::from_pmlen(configuration.value()).ok_or(SbiError::InvalidParam)?,
        )
    } else {
        return Err(SbiError::Denied);
    };
    Ok((feature, value))
}

fn sbi_configuration(
    feature: Feature,
    value: Value,
) -> core::result::Result<Configuration, SbiError> {
    let raw = match value {
        Value::State(_) if feature == Feature::Standard(FeatureType::PointerMaskingPmlen) => {
            return Err(SbiError::Failed);
        }
        Value::State(HardwareFeatureState::Disabled) => 0,
        Value::State(HardwareFeatureState::Enabled) => 1,
        Value::PointerMasking(mode)
            if feature == Feature::Standard(FeatureType::PointerMaskingPmlen) =>
        {
            mode.pmlen()
        }
        Value::PointerMasking(_) => return Err(SbiError::Failed),
    };
    Configuration::from_parts(feature, raw)
}

fn sbi_error(error: Error) -> SbiError {
    match error {
        Error::InvalidArgs => SbiError::InvalidParam,
        Error::NotSupported => SbiError::NotSupported,
        Error::AccessDenied => SbiError::Denied,
        Error::Locked => SbiError::DeniedLocked,
        _ => SbiError::Failed,
    }
}
