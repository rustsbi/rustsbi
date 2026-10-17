//! Fence transport, completion, and local execution.

use super::{Request, SbiRFence};
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
        let current = HartId::current().expect("current fence hart");
        let local = self.harts.get(current).expect("local fence queue").local();
        let targets = match sbi::ipi::target_requests(hart_mask) {
            Ok(targets) => targets,
            Err(error) => return error,
        };
        #[cfg(feature = "hypervisor")]
        if request.is_guest()
            && targets.clone().flat_map(IpiRequest::harts).any(|raw| {
                !HartId::from_raw(raw)
                    .and_then(Hart::get)
                    .is_ok_and(Hart::has_hypervisor)
            })
        {
            return SbiRet::not_supported();
        }
        let ipi = sbi::ipi().expect("published IPI adapter");
        let mut result = SbiRet::success(0);
        for hart in targets.flat_map(IpiRequest::harts) {
            let hart = HartId::from_raw(hart).expect("validated fence target");
            if hart == current {
                execute(request);
                continue;
            }
            let remote = self.harts.get(hart).expect("remote fence queue").remote();
            local.add();
            remote.set(request);
            if ipi.send_fence(hart).is_err() {
                // A receiver that already dequeued owns the acknowledgement.
                if remote.cancel(current) {
                    self.harts.get(current).unwrap().remote().sub();
                }
                result = SbiRet::failed();
                break;
            }
        }
        while !local.is_sync() {
            rfence_poll();
        }
        result
    }

    /// Drains local work after Runtime acknowledges an IPI.
    pub(in crate::sbi) fn handle(&self) {
        while self.handle_one() {}
    }

    fn handle_one(&self) -> bool {
        let current = HartId::current().expect("current fence hart");
        let local = self.harts.get(current).expect("local fence queue").local();
        if let Some((request, source)) = local.get() {
            execute(request);
            self.harts
                .get(source)
                .expect("fence source queue")
                .remote()
                .sub();
            true
        } else {
            false
        }
    }
}

/// Services requests while waiting outside the IPI handler.
///
/// Senders mark FENCE pending after enqueueing. A stale set bit only causes an
/// unnecessary dequeue attempt. The IPI handler clears the bit before draining
/// and therefore uses the unconditional handler.
#[inline]
pub(crate) fn rfence_poll() {
    if sbi::ipi().is_some_and(|ipi| ipi.fence_pending()) {
        sbi::rfence()
            .expect("published RFENCE adapter")
            .handle_one();
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
