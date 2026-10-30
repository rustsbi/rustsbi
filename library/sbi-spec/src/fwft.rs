//! Chapter 18. Firmware Features Extension (EID #0x46574654 "FWFT").
/// Extension ID for Firmware Features Extension.
#[doc(alias = "SBI_EXT_FWFT")]
pub const EID_FWFT: usize = crate::eid_from_str("FWFT") as _;

/// FWFT function identifiers.
///
/// Declared in §18.3.
#[repr(usize)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Fid {
    /// Set the firmware function configuration.
    ///
    /// Declared in §18.1.
    #[doc(alias = "SBI_EXT_FWFT_SET")]
    Set = 0,
    /// Return the firmware function configuration.
    ///
    /// Declared in §18.2.
    #[doc(alias = "SBI_EXT_FWFT_GET")]
    Get = 1,
}

impl Fid {
    /// Decodes a raw function identifier.
    pub const fn from_usize(value: usize) -> Option<Self> {
        match value {
            0 => Some(Self::Set),
            1 => Some(Self::Get),
            _ => None,
        }
    }

    /// Returns the raw function identifier.
    pub const fn as_usize(self) -> usize {
        self as usize
    }
}

/// First platform-specific local feature identifier.
pub const PLATFORM_LOCAL_START: usize = 0x4000_0000;
/// Last platform-specific local feature identifier.
pub const PLATFORM_LOCAL_END: usize = 0x7fff_ffff;
/// First platform-specific global feature identifier.
pub const PLATFORM_GLOBAL_START: usize = 0xc000_0000;
/// Last platform-specific global feature identifier.
pub const PLATFORM_GLOBAL_END: usize = 0xffff_ffff;

/// Standard FWFT feature identifiers.
///
/// Declared in §18.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FeatureType {
    /// Control misaligned access exception delegation.
    MisalignedExcDeleg = 0,
    /// Control landing pad support.
    LandingPad = 1,
    /// Control shadow stack support.
    ShadowStack = 2,
    /// Control double trap support.
    DoubleTrap = 3,
    /// Control hardware updating of PTE A/D bits.
    PteAdHwUpdating = 4,
    /// Control the pointer masking tag length.
    PointerMaskingPmlen = 5,
}

impl FeatureType {
    /// Returns the standard feature for its ABI identifier.
    pub const fn from_id(id: u32) -> Option<Self> {
        match id {
            0 => Some(Self::MisalignedExcDeleg),
            1 => Some(Self::LandingPad),
            2 => Some(Self::ShadowStack),
            3 => Some(Self::DoubleTrap),
            4 => Some(Self::PteAdHwUpdating),
            5 => Some(Self::PointerMaskingPmlen),
            _ => None,
        }
    }

    /// Returns the ABI identifier.
    pub const fn id(self) -> u32 {
        self as u32
    }
}

use crate::binary::Error;

/// A validated standard or platform-specific firmware feature identifier.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Feature {
    /// A standard firmware feature.
    Standard(FeatureType),
    /// A platform-specific local or global feature identifier.
    PlatformSpecific(u32),
}

impl Feature {
    /// Decodes a 32-bit identifier, rejecting reserved identifiers.
    pub const fn from_id(id: u32) -> Result<Self, Error> {
        if let Some(feature) = FeatureType::from_id(id) {
            return Ok(Self::Standard(feature));
        }
        match id as usize {
            PLATFORM_LOCAL_START..=PLATFORM_LOCAL_END
            | PLATFORM_GLOBAL_START..=PLATFORM_GLOBAL_END => Ok(Self::PlatformSpecific(id)),
            _ => Err(Error::Denied),
        }
    }

    /// Returns the 32-bit identifier.
    pub const fn id(self) -> u32 {
        match self {
            Self::Standard(feature) => feature.id(),
            Self::PlatformSpecific(id) => id,
        }
    }

    /// Returns whether this selects a global platform feature.
    pub const fn is_global(self) -> bool {
        matches!(self, Self::PlatformSpecific(id) if id as usize >= PLATFORM_GLOBAL_START)
    }

    /// Returns whether this standard feature carries a boolean value.
    pub const fn is_boolean(self) -> bool {
        matches!(
            self,
            Self::Standard(
                FeatureType::MisalignedExcDeleg
                    | FeatureType::LandingPad
                    | FeatureType::ShadowStack
                    | FeatureType::DoubleTrap
                    | FeatureType::PteAdHwUpdating
            )
        )
    }
}

impl TryFrom<u32> for Feature {
    type Error = Error;
    fn try_from(id: u32) -> Result<Self, Error> {
        Self::from_id(id)
    }
}

impl From<Feature> for u32 {
    fn from(feature: Feature) -> u32 {
        feature.id()
    }
}

/// A validated configuration of one firmware feature.
///
/// Standard boolean features accept only `0` and `1`. Pointer-masking and
/// platform-specific values are passed through; whether the hardware supports a
/// requested value is the service implementation's decision.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Configuration {
    feature: Feature,
    value: usize,
}

impl Configuration {
    /// Decodes a `value` ABI parameter for the given feature.
    #[inline]
    pub const fn from_parts(feature: Feature, value: usize) -> Result<Self, Error> {
        if feature.is_boolean() && value > 1 {
            return Err(Error::InvalidParam);
        }
        Ok(Self { feature, value })
    }

    /// Returns the feature selected by this configuration.
    #[inline]
    pub const fn feature(self) -> Feature {
        self.feature
    }

    /// Returns the `value` ABI parameter for this configuration.
    #[inline]
    pub const fn value(self) -> usize {
        self.value
    }
}

/// The requested locking behavior of a FWFT set operation.
///
/// The ABI defines only the least significant flag bit as `LOCK`; all other
/// bits are reserved and must be zero. There is no operation that removes a
/// lock.
#[repr(usize)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Lock {
    /// Leave the feature's lock state unchanged.
    #[default]
    Unchanged = 0,
    /// Lock the feature after this set operation.
    #[doc(alias = "SBI_FWFT_SET_FLAG_LOCK")]
    Lock = 1 << 0,
}

impl Lock {
    /// Decodes the `flags` ABI parameter.
    ///
    /// Only `0` ([`Lock::Unchanged`]) and the `LOCK` bit ([`Lock::Lock`]) are
    /// accepted. Any other value, including reserved bits, returns
    /// [`Error::InvalidParam`].
    #[inline]
    pub const fn from_flags(flags: usize) -> Result<Self, Error> {
        match flags {
            0 => Ok(Self::Unchanged),
            1 => Ok(Self::Lock),
            _ => Err(Error::InvalidParam),
        }
    }

    /// Returns the `flags` ABI parameter for this lock request.
    #[inline]
    pub const fn bits(self) -> usize {
        self as usize
    }
}

impl TryFrom<usize> for Lock {
    type Error = Error;

    #[inline]
    fn try_from(flags: usize) -> Result<Self, Error> {
        Self::from_flags(flags)
    }
}

impl From<Lock> for usize {
    #[inline]
    fn from(lock: Lock) -> usize {
        lock.bits()
    }
}
