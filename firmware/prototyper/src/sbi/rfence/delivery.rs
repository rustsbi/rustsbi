//! Fence transport, completion, and local execution.

use super::{Request, SbiRFence, queue::LocalRFenceCell};
use crate::sbi::{
    self,
    pmu::{FirmwareEvent, SbiPmu},
};
#[cfg(feature = "hypervisor")]
use runtime::hart::Hart;
use runtime::{
    fence,
    hart::HartId,
    ipi::IpiRequest,
    rustsbi::{HartMask, SbiRet},
};

impl SbiRFence {
    pub(super) fn send(&self, request: Request, hart_mask: HartMask) -> SbiRet {
        match self.send_request(request, hart_mask) {
            Ok(()) => SbiRet::success(0),
            Err(error) => error,
        }
    }

    fn send_request(&self, request: Request, hart_mask: HartMask) -> Result<(), SbiRet> {
        let current = HartId::current().map_err(SbiRet::from)?;
        let local = self.harts.get(current).map_err(SbiRet::from)?.local();
        debug_assert!(
            local.all_completed(),
            "RFENCE calls on one hart cannot overlap"
        );
        let targets = sbi::ipi::resolve_ipi_targets(hart_mask)?;
        #[cfg(feature = "hypervisor")]
        if request.is_guest()
            && targets
                .clone()
                .flat_map(IpiRequest::harts)
                .any(|hart| !Hart::get(hart).is_ok_and(Hart::has_hypervisor))
        {
            return Err(SbiRet::not_supported());
        }
        let ipi = sbi::ipi().ok_or_else(SbiRet::failed)?;
        let mut result = Ok(());
        for hart in targets.flat_map(IpiRequest::harts) {
            if hart == current {
                execute(request);
                continue;
            }
            // All records were initialized before this service was published.
            let remote = self
                .harts
                .get(hart)
                .expect("initialized RFENCE records")
                .remote();
            local.expect_completion();
            while !remote.try_enqueue(request, current) {
                self.handle_one(&local);
                core::hint::spin_loop();
            }
            if ipi.send_fence(hart).is_err() {
                // A receiver that already dequeued owns the acknowledgement.
                if remote.cancel_request_from(current) {
                    local.cancel_completion();
                }
                result = Err(SbiRet::failed());
                break;
            }
        }
        while !local.all_completed() {
            self.handle_one(&local);
            core::hint::spin_loop();
        }
        result
    }

    /// Drains local work after Runtime acknowledges an IPI.
    pub(in crate::sbi) fn handle(&self, current: HartId) {
        let local = self
            .harts
            .get(current)
            .expect("initialized RFENCE records")
            .local();
        while self.handle_one(&local) {}
    }

    /// Services the queue directly so waiting never consumes supervisor IPI events.
    fn handle_one(&self, local: &LocalRFenceCell<'_>) -> bool {
        let Some((request, source)) = local.pop_request() else {
            return false;
        };
        execute(request);
        self.harts
            .get(source)
            .expect("initialized RFENCE records")
            .remote()
            .complete_request();
        true
    }
}

/// Executes one local request and records its received PMU event.
fn execute(request: Request) {
    match request {
        Request::FenceI => {
            SbiPmu::record_event(FirmwareEvent::FenceIReceived);
            fence::fence_i();
        }
        Request::SFenceVma(range) => {
            SbiPmu::record_event(FirmwareEvent::SfenceVmaReceived);
            range.for_each(fence::sfence_vma_all, fence::sfence_vma_addr);
        }
        Request::SFenceVmaAsid { range, asid } => {
            SbiPmu::record_event(FirmwareEvent::SfenceVmaAsidReceived);
            range.for_each(
                || fence::sfence_vma_asid(asid),
                |addr| fence::sfence_vma_addr_asid(addr, asid),
            );
        }
        #[cfg(feature = "hypervisor")]
        Request::HFenceGvmaVmid { range, vmid } => {
            SbiPmu::record_event(FirmwareEvent::HfenceGvmaVmidReceived);
            range.for_each(
                || fence::hfence_gvma_vmid(vmid),
                |addr| fence::hfence_gvma_addr_vmid(addr, vmid),
            );
        }
        #[cfg(feature = "hypervisor")]
        Request::HFenceGvma(range) => {
            SbiPmu::record_event(FirmwareEvent::HfenceGvmaReceived);
            range.for_each(fence::hfence_gvma_all, fence::hfence_gvma_addr);
        }
        #[cfg(feature = "hypervisor")]
        Request::HFenceVvmaAsid { range, asid, vmid } => {
            SbiPmu::record_event(FirmwareEvent::HfenceVvmaAsidReceived);
            range.for_each(
                || fence::hfence_vvma_asid(asid, vmid),
                |addr| fence::hfence_vvma_addr_asid(addr, asid, vmid),
            );
        }
        #[cfg(feature = "hypervisor")]
        Request::HFenceVvma { range, vmid } => {
            SbiPmu::record_event(FirmwareEvent::HfenceVvmaReceived);
            range.for_each(
                || fence::hfence_vvma_all(vmid),
                |addr| fence::hfence_vvma_addr(addr, vmid),
            );
        }
    }
}
