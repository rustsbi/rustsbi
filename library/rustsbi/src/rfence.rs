use sbi_spec::binary::{HartMask, SbiRet};

/// Remote Fence support extension.
///
/// The remote fence function acts as a full TLB flush if
/// - `start_addr` and `size` are both 0, or
/// - `size` is equal to `usize::MAX`.
pub trait Rfence {
    /// Instructs remote harts to execute `FENCE.I` instruction.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code               | Description
    /// |:--------------------------|:----------------------------------------------
    /// | `SbiRet::success()`       | A remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::invalid_param()` | At least one hartid constructed from `hart_mask`, is not valid, i.e. either the hartid is not enabled by the platform or is not available to the supervisor.
    /// | `SbiRet::failed()`        | The request failed for unspecified or unknown other reasons.
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet;
    /// Instructs the remote harts to execute one or more `SFENCE.VMA` instructions,
    /// covering the range of virtual addresses between `start_addr` and `size`.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | A remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
    /// | `SbiRet::invalid_param()`   | At least one hartid constructed from `hart_mask`, is not valid, i.e. either the hartid is not enabled by the platform or is not available to the supervisor.
    /// | `SbiRet::failed()`          | The request failed for unspecified or unknown other reasons.
    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet;
    /// Instruct the remote harts to execute one or more `SFENCE.VMA` instructions,
    /// covering the range of virtual addresses between `start_addr` and `size`.
    /// This covers only the given address space by `asid`.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | A remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
    /// | `SbiRet::invalid_param()`   | Either `asid`, or at least one hartid constructed from `hart_mask`, is not valid, i.e. either the hartid is not enabled by the platform or is not available to the supervisor.
    /// | `SbiRet::failed()`          | The request failed for unspecified or unknown other reasons.
    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet;
    /// Instruct the remote harts to execute one or more `HFENCE.GVMA` instructions,
    /// covering the range of guest physical addresses between `start_addr` and `size`
    /// only for the given virtual machine by `vmid`.
    ///
    /// This function call is only valid on harts implementing the RISC-V hypervisor extension.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | A remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::not_supported()`   | This function is not supported as it is not implemented or one of the target harts do not support the RISC-V hypervisor extension.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
    /// | `SbiRet::invalid_param()`   | Either `vmid`, or at least one hartid constructed from `hart_mask`, is not valid, i.e. either the hartid is not enabled by the platform or is not available to the supervisor.
    /// | `SbiRet::failed()`          | The request failed for unspecified or unknown other reasons.
    #[inline]
    fn remote_hfence_gvma_vmid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        vmid: usize,
    ) -> SbiRet {
        let _ = (hart_mask, start_addr, size, vmid);
        SbiRet::not_supported()
    }
    /// Instruct the remote harts to execute one or more `HFENCE.GVMA` instructions,
    /// covering the range of guest physical addresses between `start_addr` and `size`
    /// for all the guests.
    ///
    /// This function call is only valid on harts implementing the RISC-V hypervisor extension.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | A remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::not_supported()`   | This function is not supported as it is not implemented or one of the target harts do not support the RISC-V hypervisor extension.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
    /// | `SbiRet::invalid_param()`   | At least one hartid constructed from `hart_mask`, is not valid, i.e. either the hartid is not enabled by the platform or is not available to the supervisor.
    /// | `SbiRet::failed()`          | The request failed for unspecified or unknown other reasons.
    #[inline]
    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        let _ = (hart_mask, start_addr, size);
        SbiRet::not_supported()
    }
    /// Instruct the remote harts to execute one or more `HFENCE.VVMA` instructions,
    /// covering the range of guest virtual addresses between `start_addr` and `size` for the given
    /// address space by `asid` and current virtual machine (by `vmid` in `hgatp` CSR)
    /// of calling hart.
    ///
    /// This function call is only valid on harts implementing the RISC-V hypervisor extension.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | A remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::not_supported()`   | This function is not supported as it is not implemented or one of the target harts do not support the RISC-V hypervisor extension.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
    /// | `SbiRet::invalid_param()`   | Either `asid`, or at least one hartid constructed from `hart_mask`, is not valid, i.e. either the hartid is not enabled by the platform or is not available to the supervisor.
    /// | `SbiRet::failed()`          | The request failed for unspecified or unknown other reasons.
    #[inline]
    fn remote_hfence_vvma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        let _ = (hart_mask, start_addr, size, asid);
        SbiRet::not_supported()
    }
    /// Instruct the remote harts to execute one or more `HFENCE.VVMA` instructions,
    /// covering the range of guest virtual addresses between `start_addr` and `size`
    /// for current virtual machine (by `vmid` in `hgatp` CSR) of calling hart.
    ///
    /// This function call is only valid on harts implementing the RISC-V hypervisor extension.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | A remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::not_supported()`   | This function is not supported as it is not implemented or one of the target harts do not support the RISC-V hypervisor extension.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
    /// | `SbiRet::invalid_param()`   | At least one hartid constructed from `hart_mask`, is not valid, i.e. either the hartid is not enabled by the platform or is not available to the supervisor.
    /// | `SbiRet::failed()`          | The request failed for unspecified or unknown other reasons.
    #[inline]
    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        let _ = (hart_mask, start_addr, size);
        SbiRet::not_supported()
    }
    /// Function internal to macros. Do not use.
    #[doc(hidden)]
    #[inline]
    fn _rustsbi_probe(&self) -> usize {
        sbi_spec::base::UNAVAILABLE_EXTENSION.wrapping_add(1)
    }
}

impl<T: Rfence> Rfence for &T {
    #[inline]
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet {
        T::remote_fence_i(self, hart_mask)
    }
    #[inline]
    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        T::remote_sfence_vma(self, hart_mask, start_addr, size)
    }
    #[inline]
    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        T::remote_sfence_vma_asid(self, hart_mask, start_addr, size, asid)
    }
    #[inline]
    fn remote_hfence_gvma_vmid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        vmid: usize,
    ) -> SbiRet {
        T::remote_hfence_gvma_vmid(self, hart_mask, start_addr, size, vmid)
    }
    #[inline]
    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        T::remote_hfence_gvma(self, hart_mask, start_addr, size)
    }
    #[inline]
    fn remote_hfence_vvma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        T::remote_hfence_vvma_asid(self, hart_mask, start_addr, size, asid)
    }
    #[inline]
    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        T::remote_hfence_vvma(self, hart_mask, start_addr, size)
    }
}

impl<T: Rfence> Rfence for Option<T> {
    #[inline]
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::remote_fence_i(inner, hart_mask)
        })
    }
    #[inline]
    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::remote_sfence_vma(inner, hart_mask, start_addr, size)
        })
    }
    #[inline]
    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::remote_sfence_vma_asid(inner, hart_mask, start_addr, size, asid)
        })
    }
    #[inline]
    fn remote_hfence_gvma_vmid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        vmid: usize,
    ) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::remote_hfence_gvma_vmid(inner, hart_mask, start_addr, size, vmid)
        })
    }
    #[inline]
    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::remote_hfence_gvma(inner, hart_mask, start_addr, size)
        })
    }
    #[inline]
    fn remote_hfence_vvma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::remote_hfence_vvma_asid(inner, hart_mask, start_addr, size, asid)
        })
    }
    #[inline]
    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        self.as_ref().map_or(SbiRet::not_supported(), |inner| {
            T::remote_hfence_vvma(inner, hart_mask, start_addr, size)
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
