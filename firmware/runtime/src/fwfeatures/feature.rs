//! Feature identities, semantic values, and hardware requirements.

use super::{FeatureState, PointerMasking};
use crate::hart::{Hart, IsaExtensions};
use csr::machine::TargetPrivilege;

/// A supervisor feature configurable by firmware.
pub use crate::rustsbi::spec::fwft::FeatureType as Feature;

/// Number of configurable supervisor features.
pub(super) const COUNT: usize = 6;

/// Configurable supervisor features in ABI identifier order.
pub(super) const ALL: [Feature; COUNT] = [
    Feature::MisalignedExcDeleg,
    Feature::LandingPad,
    Feature::ShadowStack,
    Feature::DoubleTrap,
    Feature::PteAdHwUpdating,
    Feature::PointerMaskingPmlen,
];

/// Firmware policy for PTE A/D hardware updating.
///
/// This matches OpenSBI's `fwft_adue_supported`: FWFT may control ADUE only
/// when both Svade and Svadu are advertised. Svadu without Svade still requires
/// hardware updates, but a zero FWFT reset value would not be valid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PteAdPolicy {
    /// Both Svade and Svadu are present; FWFT owns ADUE and resets it to zero.
    SupervisorControlled,
    /// Only Svadu is present; hardware must update A/D bits, so ADUE stays one.
    HardwareEnabled,
    /// Svade alone, neither extension, or no supervisor mode: no control needed.
    Unavailable,
}

impl PteAdPolicy {
    pub(super) fn of(extensions: IsaExtensions) -> Self {
        match (
            extensions.contains(IsaExtensions::SVADE),
            extensions.contains(IsaExtensions::SVADU),
        ) {
            (true, true) => Self::SupervisorControlled,
            (false, true) => Self::HardwareEnabled,
            _ => Self::Unavailable,
        }
    }

    /// Returns the policy for a hart that can run supervisor software.
    pub(super) fn for_hart(hart: &Hart) -> Self {
        if hart.supports_privilege(TargetPrivilege::Supervisor) {
            Self::of(*hart.extensions())
        } else {
            Self::Unavailable
        }
    }

    /// ADUE value Runtime must maintain even when FWFT does not expose the feature.
    pub(super) fn implicit_adue(self) -> Option<FeatureState> {
        match self {
            Self::HardwareEnabled => Some(FeatureState::Enabled),
            Self::SupervisorControlled | Self::Unavailable => None,
        }
    }
}

/// Runtime-only policy for features shared with the SBI ABI.
pub(super) trait FeatureExt {
    fn supported_by(self, hart: &Hart) -> bool;

    fn supported_by_extensions(self, extensions: IsaExtensions) -> bool;

    fn default_value(self, extensions: &IsaExtensions) -> Value;

    fn accepts(self, value: Value) -> bool;
}

impl FeatureExt for Feature {
    fn supported_by(self, hart: &Hart) -> bool {
        hart.supports_privilege(TargetPrivilege::Supervisor)
            && self.supported_by_extensions(*hart.extensions())
    }

    fn supported_by_extensions(self, extensions: IsaExtensions) -> bool {
        match self {
            Self::MisalignedExcDeleg => true,
            Self::LandingPad => extensions.contains(IsaExtensions::ZICFILP),
            Self::ShadowStack => extensions.contains(IsaExtensions::ZICFISS),
            Self::DoubleTrap => extensions.contains(IsaExtensions::SSDBLTRP),
            Self::PteAdHwUpdating => {
                PteAdPolicy::of(extensions) == PteAdPolicy::SupervisorControlled
            }
            Self::PointerMaskingPmlen => {
                cfg!(target_pointer_width = "64") && extensions.contains(IsaExtensions::SMNPM)
            }
        }
    }

    fn default_value(self, _extensions: &IsaExtensions) -> Value {
        // SBI v3 requires zero resets for the boolean features it specifies,
        // including PTE_AD_HW_UPDATING. Svadu-only harts keep ADUE enabled
        // through Runtime's implicit policy instead of exposing FWFT control.
        // Misaligned delegation has an implementation-defined reset; Runtime
        // preserves its established zero default.
        match self {
            Self::PointerMaskingPmlen => Value::PointerMasking(PointerMasking::Disabled),
            _ => Value::State(FeatureState::Disabled),
        }
    }

    fn accepts(self, value: Value) -> bool {
        matches!(
            (self, value),
            (Self::PointerMaskingPmlen, Value::PointerMasking(_))
        ) || (self != Self::PointerMaskingPmlen && matches!(value, Value::State(_)))
    }
}

/// A semantic feature value, independent of CSR field encodings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    /// Enable or disable the feature.
    State(FeatureState),
    /// Select a supported pointer masking length.
    PointerMasking(PointerMasking),
}

#[cfg(test)]
mod tests {
    use super::*;

    const PTE_AD: Feature = Feature::PteAdHwUpdating;

    const PTE_AD_COMBINATIONS: [IsaExtensions; 4] = [
        IsaExtensions::empty(),
        IsaExtensions::SVADE,
        IsaExtensions::SVADU,
        IsaExtensions::SVADE.union(IsaExtensions::SVADU),
    ];

    #[test]
    fn pte_ad_fwft_control_requires_both_svade_and_svadu() {
        for (extensions, expected_policy, expected_support) in [
            (PTE_AD_COMBINATIONS[0], PteAdPolicy::Unavailable, false),
            (PTE_AD_COMBINATIONS[1], PteAdPolicy::Unavailable, false),
            (PTE_AD_COMBINATIONS[2], PteAdPolicy::HardwareEnabled, false),
            (
                PTE_AD_COMBINATIONS[3],
                PteAdPolicy::SupervisorControlled,
                true,
            ),
        ] {
            assert_eq!(
                PteAdPolicy::of(extensions),
                expected_policy,
                "{extensions:?}",
            );
            assert_eq!(
                PTE_AD.supported_by_extensions(extensions),
                expected_support,
                "{extensions:?}",
            );
        }
    }

    #[test]
    fn pte_ad_reset_is_zero_for_every_extension_combination() {
        for extensions in PTE_AD_COMBINATIONS {
            assert_eq!(
                PTE_AD.default_value(&extensions),
                Value::State(FeatureState::Disabled),
                "{extensions:?}",
            );
        }
    }

    #[test]
    fn svadu_only_keeps_an_implicit_adue_value() {
        for (extensions, expected) in [
            (PTE_AD_COMBINATIONS[0], None),
            (PTE_AD_COMBINATIONS[1], None),
            (PTE_AD_COMBINATIONS[2], Some(FeatureState::Enabled)),
            (PTE_AD_COMBINATIONS[3], None),
        ] {
            assert_eq!(
                PteAdPolicy::of(extensions).implicit_adue(),
                expected,
                "{extensions:?}",
            );
        }
    }

    #[test]
    fn unrelated_extensions_do_not_change_the_pte_ad_policy() {
        let unrelated =
            IsaExtensions::SSTC | IsaExtensions::H | IsaExtensions::ZICFILP | IsaExtensions::SMNPM;
        for (extensions, expected) in [
            (unrelated, PteAdPolicy::Unavailable),
            (unrelated | IsaExtensions::SVADE, PteAdPolicy::Unavailable),
            (
                unrelated | IsaExtensions::SVADU,
                PteAdPolicy::HardwareEnabled,
            ),
            (
                unrelated | IsaExtensions::SVADE | IsaExtensions::SVADU,
                PteAdPolicy::SupervisorControlled,
            ),
        ] {
            assert_eq!(PteAdPolicy::of(extensions), expected, "{extensions:?}");
        }
    }

    #[test]
    fn other_features_keep_their_extension_requirements() {
        for (feature, extension) in [
            (Feature::LandingPad, IsaExtensions::ZICFILP),
            (Feature::ShadowStack, IsaExtensions::ZICFISS),
            (Feature::DoubleTrap, IsaExtensions::SSDBLTRP),
        ] {
            assert!(
                !feature.supported_by_extensions(IsaExtensions::empty()),
                "{feature:?}",
            );
            assert!(feature.supported_by_extensions(extension), "{feature:?}",);
        }
        assert!(Feature::MisalignedExcDeleg.supported_by_extensions(IsaExtensions::empty()));
        assert_eq!(
            Feature::PointerMaskingPmlen.supported_by_extensions(IsaExtensions::SMNPM),
            cfg!(target_pointer_width = "64"),
        );
        for extensions in PTE_AD_COMBINATIONS {
            assert!(!Feature::PointerMaskingPmlen.supported_by_extensions(extensions));
        }
    }

    #[test]
    fn all_boolean_features_keep_the_zero_default() {
        for extensions in PTE_AD_COMBINATIONS {
            for feature in ALL {
                let expected = if feature == Feature::PointerMaskingPmlen {
                    Value::PointerMasking(PointerMasking::Disabled)
                } else {
                    Value::State(FeatureState::Disabled)
                };
                assert_eq!(
                    feature.default_value(&extensions),
                    expected,
                    "{feature:?} with {extensions:?}",
                );
            }
        }
    }
}
