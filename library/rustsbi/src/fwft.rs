//! Typed interface for the SBI Firmware Features (FWFT) extension.
//!
//! The dispatcher decodes the raw ABI `feature`, `value` and `flags` words into
//! the types defined in `sbi-spec`. Implementations therefore never handle malformed
//! standard feature values or reserved feature identifiers.
//!
//! Standard feature identifiers and flag bits are defined in [`sbi_spec::fwft`].

pub use sbi_spec::fwft::{Configuration, Feature, Lock};

pub use sbi_spec::binary::Error;

/// SBI Firmware Features (FWFT) support extension.
///
/// Implementations receive validated configuration values and return typed
/// results. They may use the `?` operator directly instead of converting a
/// [`Result`] into [`SbiRet`](sbi_spec::binary::SbiRet).
pub trait Fwft {
    /// Set the configuration of a firmware feature.
    ///
    /// `lock` requests whether the feature must be locked after this operation.
    /// Setting the current value must succeed even when the feature is locked.
    /// Return [`Error::DeniedLocked`] when a locked feature would change value.
    /// There is no operation that unlocks a feature.
    fn set(&self, configuration: Configuration, lock: Lock) -> Result<(), Error>;

    /// Get the current configuration of a firmware feature.
    ///
    /// The returned configuration must select the requested `feature`. The
    /// dispatcher rejects a mismatched configuration with
    /// [`SbiRet::failed`](sbi_spec::binary::SbiRet::failed).
    fn get(&self, feature: Feature) -> Result<Configuration, Error>;

    /// Function internal to macros. Do not use.
    #[doc(hidden)]
    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1)
    }
}

impl<T: Fwft> Fwft for &T {
    #[inline]
    fn set(&self, configuration: Configuration, lock: Lock) -> Result<(), Error> {
        T::set(self, configuration, lock)
    }

    #[inline]
    fn get(&self, feature: Feature) -> Result<Configuration, Error> {
        T::get(self, feature)
    }
}

impl<T: Fwft> Fwft for Option<T> {
    #[inline]
    fn set(&self, configuration: Configuration, lock: Lock) -> Result<(), Error> {
        match self {
            Some(inner) => T::set(inner, configuration, lock),
            None => Err(Error::NotSupported),
        }
    }

    #[inline]
    fn get(&self, feature: Feature) -> Result<Configuration, Error> {
        match self {
            Some(inner) => T::get(inner, feature),
            None => Err(Error::NotSupported),
        }
    }

    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        match self {
            Some(_) => sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1),
            None => sbi_spec::base::UNAVAILABLE_EXTENSION,
        }
    }
}
