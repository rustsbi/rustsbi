//! Remote-fence protocol adapter.

use machine::{RemoteFence as MachineRemoteFence, RemoteFenceError};
use rustsbi::{HartMask, SbiRet};
use sbi_spec::binary::Error;

use super::ipi::targets;
use super::response;

pub(super) struct Rfence {
    fence: MachineRemoteFence,
}

impl Rfence {
    pub(super) fn new(fence: MachineRemoteFence) -> Self {
        Self { fence }
    }
}

impl rustsbi::Fence for Rfence {
    fn remote_fence_i(&self, mask: HartMask) -> SbiRet {
        result(self.fence.fence_i(targets(mask)))
    }

    fn remote_sfence_vma(&self, mask: HartMask, start: usize, size: usize) -> SbiRet {
        result(self.fence.sfence_vma(targets(mask), start, size))
    }

    fn remote_sfence_vma_asid(
        &self,
        mask: HartMask,
        start: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        result(self.fence.sfence_vma_asid(targets(mask), start, size, asid))
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_gvma_vmid(
        &self,
        mask: HartMask,
        start: usize,
        size: usize,
        vmid: usize,
    ) -> SbiRet {
        result(
            self.fence
                .hfence_gvma_vmid(targets(mask), start, size, vmid),
        )
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_gvma(&self, mask: HartMask, start: usize, size: usize) -> SbiRet {
        result(self.fence.hfence_gvma(targets(mask), start, size))
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_vvma_asid(
        &self,
        mask: HartMask,
        start: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        result(
            self.fence
                .hfence_vvma_asid(targets(mask), start, size, asid),
        )
    }

    #[cfg(feature = "hypervisor")]
    fn remote_hfence_vvma(&self, mask: HartMask, start: usize, size: usize) -> SbiRet {
        result(self.fence.hfence_vvma(targets(mask), start, size))
    }
}

fn result(result: Result<(), RemoteFenceError>) -> SbiRet {
    response(result.map(|()| 0).map_err(|error| match error {
        RemoteFenceError::InvalidHart => Error::InvalidParam,
        RemoteFenceError::InvalidAddress => Error::InvalidAddress,
        RemoteFenceError::NotSupported => Error::NotSupported,
        RemoteFenceError::Failed => Error::Failed,
    }))
}
