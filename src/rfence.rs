use crate::ecall::SbiRet;
use crate::hart_mask::HartMask;
use crate::util::OnceFatBox;
use alloc::boxed::Box;

/// Remote fence support
///
/// The remote fence function acts as a full TLB flush if
/// - `start_addr` and `size` are both 0, or
/// - `size` is equal to `usize::MAX`.
pub trait Rfence: Send {
    /// Instructs remote harts to execute `FENCE.I` instruction.
    ///
    /// # Return value
    ///
    /// Returns `SBI_SUCCESS` when remote fence was sent to all the targeted harts successfully.
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet;
    /// Instructs the remote harts to execute one or more `SFENCE.VMA` instructions,
    /// covering the range of virtual addresses between `start_addr` and `size`.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code               | Description
    /// |:--------------------------|:----------------------------------------------
    /// | SBI_SUCCESS               | Remote fence was sent to all the targeted harts successfully.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` or `size` is not valid.
    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet;
    /// Instruct the remote harts to execute one or more `SFENCE.VMA` instructions,
    /// covering the range of virtual addresses between `start_addr` and `size`.
    /// This covers only the given address space by `asid`.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code               | Description
    /// |:--------------------------|:----------------------------------------------
    /// | SBI_SUCCESS               | Remote fence was sent to all the targeted harts successfully.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` or `size` is not valid.
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
    /// | Return code               | Description
    /// |:--------------------------|:----------------------------------------------
    /// | SBI_SUCCESS               | Remote fence was sent to all the targeted harts successfully.
    /// | SBI_ERR_NOT_SUPPORTED     | This function is not supported as it is not implemented or one of the target hart doesn’t support hypervisor extension.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` or `size` is not valid.
    fn remote_hfence_gvma_vmid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        vmid: usize,
    ) -> SbiRet {
        drop((hart_mask, start_addr, size, vmid));
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
    /// | Return code               | Description
    /// |:--------------------------|:----------------------------------------------
    /// | SBI_SUCCESS               | Remote fence was sent to all the targeted harts successfully.
    /// | SBI_ERR_NOT_SUPPORTED     | This function is not supported as it is not implemented or one of the target hart does not support hypervisor extension.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` or `size` is not valid.
    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        drop((hart_mask, start_addr, size));
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
    /// | Return code               | Description
    /// |:--------------------------|:----------------------------------------------
    /// | SBI_SUCCESS               | Remote fence was sent to all the targeted harts successfully.
    /// | SBI_ERR_NOT_SUPPORTED     | This function is not supported as it is not implemented or one of the target hart does not support hypervisor extension.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` or `size` is not valid.
    fn remote_hfence_vvma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        drop((hart_mask, start_addr, size, asid));
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
    /// | Return code               | Description
    /// |:--------------------------|:----------------------------------------------
    /// | SBI_SUCCESS               | Remote fence was sent to all the targeted harts successfully.
    /// | SBI_ERR_NOT_SUPPORTED     | This function is not supported as it is not implemented or one of the target hart doesn’t support hypervisor extension.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` or `size` is not valid.
    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        drop((hart_mask, start_addr, size));
        SbiRet::not_supported()
    }
}

static RFENCE: OnceFatBox<dyn Rfence + Sync + 'static> = OnceFatBox::new();

#[doc(hidden)] // use through a macro
pub fn init_rfence<T: Rfence + Sync + 'static>(rfence: T) {
    let result = RFENCE.set(Box::new(rfence));
    if result.is_err() {
        panic!("load sbi module when already loaded")
    }
}

#[inline]
pub(crate) fn probe_rfence() -> bool {
    RFENCE.get().is_some()
}

pub(crate) fn remote_fence_i(hart_mask: HartMask) -> SbiRet {
    if let Some(rfence) = RFENCE.get() {
        rfence.remote_fence_i(hart_mask)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_sfence_vma(hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
    if let Some(rfence) = RFENCE.get() {
        rfence.remote_sfence_vma(hart_mask, start_addr, size)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_sfence_vma_asid(
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
    asid: usize,
) -> SbiRet {
    if let Some(rfence) = RFENCE.get() {
        rfence.remote_sfence_vma_asid(hart_mask, start_addr, size, asid)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_hfence_gvma_vmid(
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
    vmid: usize,
) -> SbiRet {
    if let Some(rfence) = RFENCE.get() {
        rfence.remote_hfence_gvma_vmid(hart_mask, start_addr, size, vmid)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_hfence_gvma(hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
    if let Some(rfence) = RFENCE.get() {
        rfence.remote_hfence_gvma(hart_mask, start_addr, size)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_hfence_vvma_asid(
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
    asid: usize,
) -> SbiRet {
    if let Some(rfence) = RFENCE.get() {
        rfence.remote_hfence_vvma_asid(hart_mask, start_addr, size, asid)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_hfence_vvma(hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
    if let Some(rfence) = RFENCE.get() {
        rfence.remote_hfence_vvma(hart_mask, start_addr, size)
    } else {
        SbiRet::not_supported()
    }
}
