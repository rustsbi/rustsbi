//! Firmware feature control.
//!
//! # References
//!
//! - Specification: [RISC-V SBI FWFT extension](https://github.com/riscv-non-isa/riscv-sbi-doc/blob/v3.0/src/ext-firmware-features.adoc) —
//!   feature identifiers and get/set semantics.

use runtime::features::{EnvironmentFeature, FeatureError, SupervisorEnvironment};
use runtime::rustsbi::SbiRet;
use runtime::rustsbi::spec::fwft::feature_type;

/// SBI identifiers and result translation for Runtime environment operations.
pub(crate) struct SbiFwft;

impl SbiFwft {
    fn environment_feature(feature_id: usize) -> Option<EnvironmentFeature> {
        match feature_id {
            feature_type::LANDING_PAD => Some(EnvironmentFeature::LandingPad),
            feature_type::SHADOW_STACK => Some(EnvironmentFeature::ShadowStack),
            feature_type::DOUBLE_TRAP => Some(EnvironmentFeature::DoubleTrap),
            feature_type::PTE_AD_HW_UPDATING => Some(EnvironmentFeature::PteAdHardwareUpdating),
            feature_type::POINTER_MASKING_PMLEN => Some(EnvironmentFeature::PointerMasking),
            _ => None,
        }
    }

    fn feature_error(error: FeatureError) -> SbiRet {
        match error {
            FeatureError::InvalidValue => SbiRet::invalid_param(),
            FeatureError::Unsupported => SbiRet::not_supported(),
            error => {
                warn!("Firmware feature operation failed: {:?}", error);
                SbiRet::failed()
            }
        }
    }
}

impl runtime::rustsbi::Fwft for SbiFwft {
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet {
        // The LOCK flag is not supported: locked features can never be
        // modified again, which would prevent firmware reconfiguration.
        if flags != 0 {
            return SbiRet::invalid_param();
        }
        let environment = SupervisorEnvironment::current()
            .expect("BUG: firmware feature request is outside the published hart topology");
        if feature_id as usize == feature_type::MISALIGNED_EXC_DELEG {
            return match environment.set_misaligned_delegation(value) {
                Ok(()) => SbiRet::success(0),
                Err(error) => Self::feature_error(error),
            };
        }
        let Some(feature) = Self::environment_feature(feature_id as usize) else {
            return SbiRet::not_supported();
        };
        match environment.set(feature, value) {
            Ok(()) => SbiRet::success(0),
            Err(error) => Self::feature_error(error),
        }
    }

    fn get(&self, feature_id: u32) -> SbiRet {
        let environment = SupervisorEnvironment::current()
            .expect("BUG: firmware feature request is outside the published hart topology");
        if feature_id as usize == feature_type::MISALIGNED_EXC_DELEG {
            return match environment.get_misaligned_delegation() {
                Ok(value) => SbiRet::success(value),
                Err(error) => Self::feature_error(error),
            };
        }
        let Some(feature) = Self::environment_feature(feature_id as usize) else {
            return SbiRet::not_supported();
        };
        match environment.get(feature) {
            Ok(value) => SbiRet::success(value),
            Err(error) => Self::feature_error(error),
        }
    }
}
