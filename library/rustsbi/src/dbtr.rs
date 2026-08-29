use sbi_spec::binary::{SbiRet, SharedPtr, TriggerMask};

/// SBI Debug Triggers (DBTR) support extension.
pub trait Dbtr {
    /// Get the number of debug triggers on the calling hart.
    fn num_triggers(&self, trig_tdata1: usize) -> usize;

    /// Set or disable DBTR shared memory for the calling hart.
    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet;

    /// Read debug trigger state and configuration into shared memory.
    fn read_triggers(&self, trig_idx_base: usize, trig_count: usize) -> SbiRet;

    /// Install debug triggers from shared memory.
    fn install_triggers(&self, trig_count: usize) -> SbiRet;

    /// Update debug triggers from shared memory.
    fn update_triggers(&self, trig_count: usize) -> SbiRet;

    /// Uninstall a set of debug triggers.
    fn uninstall_triggers(&self, triggers: TriggerMask) -> SbiRet;

    /// Enable a set of debug triggers.
    fn enable_triggers(&self, triggers: TriggerMask) -> SbiRet;

    /// Disable a set of debug triggers.
    fn disable_triggers(&self, triggers: TriggerMask) -> SbiRet;

    /// Function internal to macros. Do not use.
    #[doc(hidden)]
    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1)
    }
}

impl<T: Dbtr> Dbtr for &T {
    #[inline]
    fn num_triggers(&self, trig_tdata1: usize) -> usize {
        T::num_triggers(self, trig_tdata1)
    }

    #[inline]
    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet {
        T::set_shmem(self, shmem, flags)
    }

    #[inline]
    fn read_triggers(&self, trig_idx_base: usize, trig_count: usize) -> SbiRet {
        T::read_triggers(self, trig_idx_base, trig_count)
    }

    #[inline]
    fn install_triggers(&self, trig_count: usize) -> SbiRet {
        T::install_triggers(self, trig_count)
    }

    #[inline]
    fn update_triggers(&self, trig_count: usize) -> SbiRet {
        T::update_triggers(self, trig_count)
    }

    #[inline]
    fn uninstall_triggers(&self, triggers: TriggerMask) -> SbiRet {
        T::uninstall_triggers(self, triggers)
    }

    #[inline]
    fn enable_triggers(&self, triggers: TriggerMask) -> SbiRet {
        T::enable_triggers(self, triggers)
    }

    #[inline]
    fn disable_triggers(&self, triggers: TriggerMask) -> SbiRet {
        T::disable_triggers(self, triggers)
    }
}

impl<T: Dbtr> Dbtr for Option<T> {
    #[inline]
    fn num_triggers(&self, trig_tdata1: usize) -> usize {
        self.as_ref()
            .map_or(0, |inner| T::num_triggers(inner, trig_tdata1))
    }

    #[inline]
    fn set_shmem(&self, shmem: SharedPtr<u8>, flags: usize) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::set_shmem(inner, shmem, flags)
        })
    }

    #[inline]
    fn read_triggers(&self, trig_idx_base: usize, trig_count: usize) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::read_triggers(inner, trig_idx_base, trig_count)
        })
    }

    #[inline]
    fn install_triggers(&self, trig_count: usize) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::install_triggers(inner, trig_count)
        })
    }

    #[inline]
    fn update_triggers(&self, trig_count: usize) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::update_triggers(inner, trig_count)
        })
    }

    #[inline]
    fn uninstall_triggers(&self, triggers: TriggerMask) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::uninstall_triggers(inner, triggers)
        })
    }

    #[inline]
    fn enable_triggers(&self, triggers: TriggerMask) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::enable_triggers(inner, triggers)
        })
    }

    #[inline]
    fn disable_triggers(&self, triggers: TriggerMask) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::disable_triggers(inner, triggers)
        })
    }

    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        match self {
            Some(_) => sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1),
            None => sbi_spec::base::UNAVAILABLE_EXTENSION,
        }
    }
}
