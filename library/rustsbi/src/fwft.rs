use sbi_spec::binary::SbiRet;

/// SBI Firmware Features (FWFT) support extension.
pub trait Fwft {
    /// Set the configuration value of a firmware feature.
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet;

    /// Get the configuration value of a firmware feature.
    fn get(&self, feature_id: u32) -> SbiRet;

    /// Function internal to macros. Do not use.
    #[doc(hidden)]
    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1)
    }
}

impl<T: Fwft> Fwft for &T {
    #[inline]
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet {
        T::set(self, feature_id, value, flags)
    }

    #[inline]
    fn get(&self, feature_id: u32) -> SbiRet {
        T::get(self, feature_id)
    }
}

impl<T: Fwft> Fwft for Option<T> {
    #[inline]
    fn set(&self, feature_id: u32, value: usize, flags: usize) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::set(inner, feature_id, value, flags)
        })
    }

    #[inline]
    fn get(&self, feature_id: u32) -> SbiRet {
        self.as_ref()
            .map_or(SbiRet::not_supported(), |inner| T::get(inner, feature_id))
    }

    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        match self {
            Some(_) => sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1),
            None => sbi_spec::base::UNAVAILABLE_EXTENSION,
        }
    }
}
