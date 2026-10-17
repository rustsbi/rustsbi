//! Feature identities, semantic values, and hardware requirements.

use super::PointerMasking;
use crate::hart::{Hart, IsaExtensions};
use csr::machine::TargetPrivilege;

/// A supervisor feature controlled through FWFT.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Feature {
    /// Delegate misaligned loads and stores to supervisor mode.
    MisalignedDelegation,
    /// Enable supervisor landing pads.
    LandingPad,
    /// Enable supervisor shadow stacks.
    ShadowStack,
    /// Enable supervisor double-trap handling.
    DoubleTrap,
    /// Enable hardware updates of PTE accessed and dirty bits.
    PteAdUpdating,
    /// Select supervisor pointer masking length.
    PointerMasking,
}

impl Feature {
    /// Number of configurable supervisor features.
    pub const COUNT: usize = Self::ALL.len();

    pub(super) const ALL: [Self; 6] = [
        Self::MisalignedDelegation,
        Self::LandingPad,
        Self::ShadowStack,
        Self::DoubleTrap,
        Self::PteAdUpdating,
        Self::PointerMasking,
    ];

    pub(super) fn supported_by(self, hart: &Hart) -> bool {
        if !hart.supports_privilege(TargetPrivilege::Supervisor) {
            return false;
        }
        let extensions = hart.extensions();
        match self {
            Self::MisalignedDelegation => true,
            Self::LandingPad => extensions.contains(IsaExtensions::ZICFILP),
            Self::ShadowStack => extensions.contains(IsaExtensions::ZICFISS),
            Self::DoubleTrap => extensions.contains(IsaExtensions::SSDBLTRP),
            Self::PteAdUpdating => extensions.contains(IsaExtensions::SVADE | IsaExtensions::SVADU),
            Self::PointerMasking => {
                cfg!(target_pointer_width = "64") && extensions.contains(IsaExtensions::SMNPM)
            }
        }
    }

    pub(super) fn default_value(self) -> Value {
        match self {
            Self::PointerMasking => Value::PointerMasking(PointerMasking::Disabled),
            _ => Value::Enabled(false),
        }
    }

    pub(super) fn accepts(self, value: Value) -> bool {
        matches!(
            (self, value),
            (Self::PointerMasking, Value::PointerMasking(_))
        ) || (self != Self::PointerMasking && matches!(value, Value::Enabled(_)))
    }
}

/// A semantic feature value, independent of CSR field encodings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Value {
    /// Enable or disable a boolean feature.
    Enabled(bool),
    /// Select a supported pointer masking length.
    PointerMasking(PointerMasking),
}
