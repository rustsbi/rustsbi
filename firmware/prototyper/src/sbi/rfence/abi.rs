//! SBI RFENCE validation and ABI implementation.

use super::{AddressRange, Request, SbiRFence};
use crate::sbi::pmu::{FirmwareEvent, SbiPmu};
use runtime::rustsbi::{Fence, HartMask, SbiRet};
#[cfg(feature = "hypervisor")]
use runtime::{fence, hart::Hart};

impl Fence for SbiRFence {
    /// Remote instruction fence for specified harts.
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet {
        SbiPmu::record_event(FirmwareEvent::FenceISent);
        self.send(Request::FenceI, hart_mask)
    }

    /// Remote supervisor fence for virtual memory on specified harts.
    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        SbiPmu::record_event(FirmwareEvent::SfenceVmaSent);
        let range = match AddressRange::new(start_addr, size) {
            Ok(size) => size,
            Err(e) => return e,
        };

        self.send(Request::SFenceVma(range), hart_mask)
    }

    /// Remote supervisor fence for virtual memory with ASID on specified harts.
    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        SbiPmu::record_event(FirmwareEvent::SfenceVmaAsidSent);
        let range = match AddressRange::new(start_addr, size) {
            Ok(size) => size,
            Err(e) => return e,
        };

        self.send(Request::SFenceVmaAsid { range, asid }, hart_mask)
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_gvma_vmid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        vmid: usize,
    ) -> SbiRet {
        if !Hart::current().is_ok_and(Hart::has_hypervisor) {
            return SbiRet::not_supported();
        }
        SbiPmu::record_event(FirmwareEvent::HfenceGvmaVmidSent);

        let range = match AddressRange::new(start_addr, size) {
            Ok(s) => s,
            Err(e) => return e,
        };

        self.send(Request::HFenceGvmaVmid { range, vmid }, hart_mask)
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        if !Hart::current().is_ok_and(Hart::has_hypervisor) {
            return SbiRet::not_supported();
        }
        SbiPmu::record_event(FirmwareEvent::HfenceGvmaSent);

        let range = match AddressRange::new(start_addr, size) {
            Ok(s) => s,
            Err(e) => return e,
        };

        self.send(Request::HFenceGvma(range), hart_mask)
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_vvma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        if !Hart::current().is_ok_and(Hart::has_hypervisor) {
            return SbiRet::not_supported();
        }
        let vmid = match fence::current_vmid() {
            Ok(vmid) => vmid,
            Err(error) => return error.into(),
        };
        SbiPmu::record_event(FirmwareEvent::HfenceVvmaAsidSent);

        let range = match AddressRange::new(start_addr, size) {
            Ok(s) => s,
            Err(e) => return e,
        };

        self.send(Request::HFenceVvmaAsid { range, asid, vmid }, hart_mask)
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        if !Hart::current().is_ok_and(Hart::has_hypervisor) {
            return SbiRet::not_supported();
        }
        let vmid = match fence::current_vmid() {
            Ok(vmid) => vmid,
            Err(error) => return error.into(),
        };
        SbiPmu::record_event(FirmwareEvent::HfenceVvmaSent);

        let range = match AddressRange::new(start_addr, size) {
            Ok(s) => s,
            Err(e) => return e,
        };

        self.send(Request::HFenceVvma { range, vmid }, hart_mask)
    }
}
