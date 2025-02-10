use sbi_spec::binary::{SbiRet, SharedPtr};

/// Steal-time Accounting extension.
///
/// SBI implementations may encounter situations where virtual harts are ready to
/// run, but must be withheld from running. These situations may be, for example,
/// when multiple SBI domains share processors or when an SBI implementation is a
/// hypervisor and guest contexts share processors with other guest contexts or
/// host tasks. When virtual harts are at times withheld from running, observers
/// within the contexts of the virtual harts may need a way to account for less
/// progress than would otherwise be expected. The time a virtual hart was ready,
/// but had to wait, is called "stolen time" and the tracking of it is referred to
/// as steal-time accounting. The Steal-time Accounting (STA) extension defines the
/// mechanism in which an SBI implementation provides steal-time and preemption
/// information, for each virtual hart, to supervisor-mode software.
pub trait Sta {
    /// Set Steal-time Shared Memory Address.
    ///
    /// Set the shared memory physical base address for steal-time accounting of the
    /// calling virtual hart and enable the SBI implementation's steal-time information
    /// reporting.
    ///
    /// If the physical address of `shmem` is not all-ones bitwise, then the `shmem` pointer
    /// specifies the shared memory physical base address. The physical address of `shmem`
    /// MUST be 64-byte aligned. The size of the shared memory must be 64 bytes.
    /// The SBI implementation MUST zero the shared memory before returning from the SBI
    /// call.
    ///
    /// If physical address of `shmem` are all-ones bitwise, the SBI
    /// implementation will stop reporting steal-time information for the virtual hart.
    ///
    /// The `flags` parameter is reserved for future use and MUST be zero.
    ///
    /// It is not expected for the shared memory to be written by the supervisor-mode
    /// software while it is in use for steal-time accounting. However, the SBI
    /// implementation MUST not misbehave if a write operation from supervisor-mode software
    /// occurs, however, in that case, it MAY leave the shared memory filled with
    /// inconsistent data.
    ///
    /// The SBI implementation MUST stop writing to the shared memory when the
    /// supervisor-mode software is not runnable, such as upon system reset or system
    /// suspend.
    ///
    /// *NOTE:* Not writing to the shared memory when the supervisor-mode software is
    /// not runnable avoids unnecessary work and supports repeatable capture of a
    /// system image while the supervisor-mode software is suspended.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | The steal-time shared memory physical base address was set or cleared successfully.
    /// | `SbiRet::invalid_param()`   | The `flags` parameter is not zero or the `shmem` is not 64-byte aligned.
    /// | `SbiRet::invalid_address()` | The shared memory pointed to by the `shmem` parameter is not writable or does not satisfy other requirements of shared memory physical address range.
    /// | `SbiRet::failed()`          | The request failed for unspecified or unknown other reasons.
    fn set_shmem(&self, shmem: SharedPtr<[u8; 64]>, flags: usize) -> SbiRet;
    /// Function internal to macros. Do not use.
    #[doc(hidden)]
    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1)
    }
}

impl<T: Sta> Sta for &T {
    #[inline]
    fn set_shmem(&self, shmem: SharedPtr<[u8; 64]>, flags: usize) -> SbiRet {
        T::set_shmem(self, shmem, flags)
    }
}

impl<T: Sta> Sta for Option<T> {
    #[inline]
    fn set_shmem(&self, shmem: SharedPtr<[u8; 64]>, flags: usize) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::set_shmem(inner, shmem, flags)
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
