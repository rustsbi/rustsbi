use crate::hart_mask::HartMask;
use crate::ecall::SbiRet;

/// Remote fence support
///
/// In RustSBI, RFENCE support requires an IPI support is implemented.
/// If your platform does not provide IPI support, RustSBI will disable RFENCE
/// interface access from supervisor level.
///
/// The remote fence function acts as a full TLB flush if
/// - `start_addr` and `size` are both 0
/// - `size` is equal to `usize::max_value()`
pub trait Rfence: Send {
    /// Instructs remote harts to execute `FENCE.I` instruction.
    ///
    /// Returns `SBI_SUCCESS` when remote fence was sent to all the targeted harts successfully.
    fn remote_fence_i(&mut self, hart_mask: HartMask) -> SbiRet;
    /// Instructs the remote harts to execute one or more `SFENCE.VMA` instructions, 
    /// covering the range of virtual addresses between start and size.
    /// 
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code               | Description 
    /// |:--------------------------|:----------------------------------------------
    /// | SBI_SUCCESS               | Remote fence was sent to all the targeted harts successfully.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` or `size` is not valid.
    fn remote_sfence_vma(&mut self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet;
    /// Instruct the remote harts to execute one or more `SFENCE.VMA` instructions, 
    /// covering the range of virtual addresses between start and size. This covers only the given `ASID`.
    ///
    /// # Return value
    ///
    /// The possible return error codes returned in `SbiRet.error` are shown in the table below:
    ///
    /// | Return code               | Description 
    /// |:--------------------------|:----------------------------------------------
    /// | SBI_SUCCESS               | Remote fence was sent to all the targeted harts successfully.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` or `size` is not valid.
    fn remote_sfence_vma_asid(&mut self, hart_mask: HartMask, start_addr: usize, size: usize, asid: usize) -> SbiRet;
    /// Instruct the remote harts to execute one or more `HFENCE.GVMA` instructions, 
    /// covering the range of guest physical addresses between start and size only for the given `VMID`. 
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
    fn remote_hfence_gvma_vmid(&mut self, hart_mask: HartMask, start_addr: usize, size: usize, vmid: usize) -> SbiRet {
        drop((hart_mask, start_addr, size, vmid));
        SbiRet::not_supported()
    }
    /// Instruct the remote harts to execute one or more `HFENCE.GVMA` instructions, 
    /// covering the range of guest physical addresses between start and size for all the guests.
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
    fn remote_hfence_gvma(&mut self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        drop((hart_mask, start_addr, size));
        SbiRet::not_supported()
    }
    /// Instruct the remote harts to execute one or more `HFENCE.VVMA` instructions, 
    /// covering the range of guest virtual addresses between start and size for the given `ASID` and current `VMID` (in `hgatp` CSR) 
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
    /// | SBI_ERR_NOT_SUPPORTED     | This function is not supported as it is not implemented or one of the target hart doesn’t support hypervisor extension.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` or `size` is not valid.
    fn remote_hfence_vvma_asid(&mut self, hart_mask: HartMask, start_addr: usize, size: usize, asid: usize) -> SbiRet {
        drop((hart_mask, start_addr, size, asid));
        SbiRet::not_supported()
    }
    /// Instruct the remote harts to execute one or more `HFENCE.VVMA` instructions, 
    /// covering the range of guest virtual addresses between start and size for current `VMID` (in `hgatp` CSR) 
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
    /// | SBI_ERR_NOT_SUPPORTED     | This function is not supported as it is not implemented or one of the target hart doesn’t support hypervisor extension.
    /// | SBI_ERR_INVALID_ADDRESS   | `start_addr` or `size` is not valid.
    fn remote_hfence_vvma(&mut self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        drop((hart_mask, start_addr, size, size));
        SbiRet::not_supported()
    }
}

use alloc::boxed::Box;
use spin::Mutex;

lazy_static::lazy_static! {
    static ref RFENCE: Mutex<Option<Box<dyn Rfence>>> = Mutex::new(None);
}

#[doc(hidden)] // use through a macro
pub fn init_rfence<T: Rfence + Send + 'static>(rfence: T) {
    *RFENCE.lock() = Some(Box::new(rfence));
}

#[inline]
pub(crate) fn probe_rfence() -> bool {
    RFENCE.lock().as_ref().is_some()
}

pub(crate) fn remote_fence_i(hart_mask: HartMask) -> SbiRet {
    if let Some(rfence) = RFENCE.lock().as_mut() {
        rfence.remote_fence_i(hart_mask)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_sfence_vma(hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
    if let Some(rfence) = RFENCE.lock().as_mut() {
        rfence.remote_sfence_vma(hart_mask, start_addr, size)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_sfence_vma_asid(hart_mask: HartMask, start_addr: usize, size: usize, asid: usize) -> SbiRet {
    if let Some(rfence) = RFENCE.lock().as_mut() {
        rfence.remote_sfence_vma_asid(hart_mask, start_addr, size, asid)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_hfence_gvma_vmid(hart_mask: HartMask, start_addr: usize, size: usize, vmid: usize) -> SbiRet {
    if let Some(rfence) = RFENCE.lock().as_mut() {
        rfence.remote_hfence_gvma_vmid(hart_mask, start_addr, size, vmid)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_hfence_gvma(hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
    if let Some(rfence) = RFENCE.lock().as_mut() {
        rfence.remote_hfence_gvma(hart_mask, start_addr, size)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_hfence_vvma_asid(hart_mask: HartMask, start_addr: usize, size: usize, asid: usize) -> SbiRet {
    if let Some(rfence) = RFENCE.lock().as_mut() {
        rfence.remote_hfence_vvma_asid(hart_mask, start_addr, size, asid)
    } else {
        SbiRet::not_supported()
    }
}

pub(crate) fn remote_hfence_vvma(hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
    if let Some(rfence) = RFENCE.lock().as_mut() {
        rfence.remote_hfence_vvma(hart_mask, start_addr, size)
    } else {
        SbiRet::not_supported()
    }
}
