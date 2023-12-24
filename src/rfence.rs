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
    /// Returns `SbiRet::success()` when remote fence was sent to all the targeted harts successfully.
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
    /// | `SbiRet::success()`         | Remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
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
    /// | `SbiRet::success()`         | Remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
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
    /// This function call is only valid for harts implementing hypervisor extension.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | Remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::not_supported()`   | This function is not supported as it is not implemented or one of the target hart doesn’t support hypervisor extension.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
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
    /// This function call is only valid for harts implementing hypervisor extension.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | Remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::not_supported()`   | This function is not supported as it is not implemented or one of the target hart does not support hypervisor extension.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        let _ = (hart_mask, start_addr, size);
        SbiRet::not_supported()
    }
    /// Instruct the remote harts to execute one or more `HFENCE.VVMA` instructions,
    /// covering the range of guest virtual addresses between `start_addr` and `size` for the given
    /// address space by `asid` and current virtual machine (by `vmid` in `hgatp` CSR)
    /// of calling hart.
    ///
    /// This function call is only valid for harts implementing hypervisor extension.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | Remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::not_supported()`   | This function is not supported as it is not implemented or one of the target hart does not support hypervisor extension.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
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
    /// This function call is only valid for harts implementing hypervisor extension.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code                 | Description
    /// |:----------------------------|:----------------------------------------------
    /// | `SbiRet::success()`         | Remote fence was sent to all the targeted harts successfully.
    /// | `SbiRet::not_supported()`   | This function is not supported as it is not implemented or one of the target hart doesn’t support hypervisor extension.
    /// | `SbiRet::invalid_address()` | `start_addr` or `size` is not valid.
    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        let _ = (hart_mask, start_addr, size);
        SbiRet::not_supported()
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
