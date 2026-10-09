//! Validated address-translation and instruction fences on the current hart.
//!
//! Policy firmware chooses the target harts and transports requests.
//! Runtime validates each request and owns the complete local flush operation.
//!
//! Range semantics follow the [SBI RFENCE specification](https://docs.riscv.org/reference/sbi/v3.0/ext-rfence.html).

#![deny(unsafe_code)]

#[cfg(feature = "hypervisor")]
mod guest;

#[cfg(feature = "hypervisor")]
use crate::csr::{Misa, Readable};

use core::marker::PhantomData;

use crate::hart::{HartId, HartIdError};
use crate::instructions::fence;

const PAGE_SIZE: usize = 4096;

/// The address-translation domain and identifier of a fence operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FenceOperation {
    /// Synchronizes instruction fetch with prior writes on the current hart.
    FenceI,
    /// Invalidates supervisor translations for every ASID.
    SFenceVma,
    /// Invalidates supervisor translations for one ASID.
    SFenceVmaAsid {
        /// Address-space identifier.
        asid: usize,
    },
    /// Invalidates guest-physical translations for every VMID.
    #[cfg(feature = "hypervisor")]
    HFenceGvma,
    /// Invalidates guest-physical translations for one VMID.
    #[cfg(feature = "hypervisor")]
    HFenceGvmaVmid {
        /// Virtual-machine identifier.
        vmid: usize,
    },
    /// Invalidates guest-virtual translations for the requesting hart's VMID.
    #[cfg(feature = "hypervisor")]
    HFenceVvma,
    /// Invalidates guest-virtual translations for one ASID and the requesting
    /// hart's VMID.
    #[cfg(feature = "hypervisor")]
    HFenceVvmaAsid {
        /// Guest address-space identifier.
        asid: usize,
    },
}

/// An invalid request or an unsupported operation on the executing hart.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FenceError {
    /// The range starts at an address that is not page aligned.
    UnalignedAddress,
    /// The end address cannot be represented without wrapping.
    AddressOverflow,
    /// The executing hart does not implement the H extension.
    HypervisorUnavailable,
    /// The executing hart cannot represent the requesting guest's context.
    #[cfg(feature = "hypervisor")]
    GuestContextUnavailable,
    /// An architectural context access faulted.
    #[cfg(feature = "hypervisor")]
    Access(crate::trap::Error),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AddressRange {
    All,
    Pages { start: usize, size: usize },
}

impl AddressRange {
    fn new(start: usize, size: usize) -> Result<Self, FenceError> {
        // SBI RFENCE defines these values as the entire address space.
        // In particular, size == usize::MAX ignores the start address.
        if (start == 0 && size == 0) || size == usize::MAX {
            return Ok(Self::All);
        }
        if !start.is_multiple_of(PAGE_SIZE) {
            return Err(FenceError::UnalignedAddress);
        }
        start.checked_add(size).ok_or(FenceError::AddressOverflow)?;
        Ok(Self::Pages { start, size })
    }

    fn apply(self, full_flush_limit: usize, mut invalidate_fn: impl FnMut(Option<usize>)) {
        match self {
            Self::All => invalidate_fn(None),
            Self::Pages { size, .. } if size > full_flush_limit => invalidate_fn(None),
            Self::Pages { start, size } => {
                // Construction checked start + size. Every offset is below
                // size, including the final partially covered page.
                for offset in (0..size).step_by(PAGE_SIZE) {
                    invalidate_fn(Some(start + offset));
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RequestOperation {
    FenceI,
    SFenceVma,
    SFenceVmaAsid {
        asid: usize,
    },
    #[cfg(feature = "hypervisor")]
    HFenceGvma,
    #[cfg(feature = "hypervisor")]
    HFenceGvmaVmid {
        vmid: usize,
    },
    #[cfg(feature = "hypervisor")]
    HFenceVvma {
        guest_context: guest::GuestContext,
    },
    #[cfg(feature = "hypervisor")]
    HFenceVvmaAsid {
        asid: usize,
        guest_context: guest::GuestContext,
    },
}

/// A checked request that policy firmware can copy into a remote-hart queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FenceRequest {
    operation: RequestOperation,
    range: AddressRange,
}

/// Validated fence execution on the hart bound to this object.
///
/// The local executor cannot be sent to another hart. A checked
/// [`FenceRequest`] contains a transferable guest-context snapshot and may
/// be copied into policy's remote-hart queues.
///
/// # Panics
///
/// Operations panic if the executor is used on a different hart.
pub struct LocalFence {
    hart: HartId,
    _hart_local: PhantomData<*mut ()>,
}

impl LocalFence {
    /// Binds fence preparation and execution to the current published hart.
    pub fn current() -> Result<Self, HartIdError> {
        Ok(Self {
            hart: HartId::current()?,
            _hart_local: PhantomData,
        })
    }

    fn assert_current(&self) {
        assert_eq!(
            HartId::current(),
            Ok(self.hart),
            "BUG: local fence executor used on another hart"
        );
    }

    /// Checks a fence request before policy firmware sends it to target harts.
    ///
    /// `start_addr` is a byte address.
    /// Translation fences accept a page-aligned start and a byte size,
    /// including a final partial page. The SBI values `(0, 0)` and
    /// `size == usize::MAX` select the entire address space.
    /// `FenceI` ignores the address arguments.
    /// Guest-virtual fences capture this hart's guest context, including VMID,
    /// so remote execution invalidates the requesting hart's guest.
    pub fn request(
        &self,
        operation: FenceOperation,
        start_addr: usize,
        size: usize,
    ) -> Result<FenceRequest, FenceError> {
        self.assert_current();
        let range = if operation == FenceOperation::FenceI {
            AddressRange::All
        } else {
            AddressRange::new(start_addr, size)?
        };
        let operation = match operation {
            FenceOperation::FenceI => RequestOperation::FenceI,
            FenceOperation::SFenceVma => RequestOperation::SFenceVma,
            FenceOperation::SFenceVmaAsid { asid } => RequestOperation::SFenceVmaAsid { asid },
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceGvma => RequestOperation::HFenceGvma,
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceGvmaVmid { vmid } => RequestOperation::HFenceGvmaVmid { vmid },
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceVvma => RequestOperation::HFenceVvma {
                guest_context: guest::GuestContext::capture()?,
            },
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceVvmaAsid { asid } => RequestOperation::HFenceVvmaAsid {
                asid,
                guest_context: guest::GuestContext::capture()?,
            },
        };
        Ok(FenceRequest { operation, range })
    }
}

impl FenceRequest {
    /// Returns the operation and its address-space or virtual-machine ID.
    pub fn operation(self) -> FenceOperation {
        match self.operation {
            RequestOperation::FenceI => FenceOperation::FenceI,
            RequestOperation::SFenceVma => FenceOperation::SFenceVma,
            RequestOperation::SFenceVmaAsid { asid } => FenceOperation::SFenceVmaAsid { asid },
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceGvma => FenceOperation::HFenceGvma,
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceGvmaVmid { vmid } => FenceOperation::HFenceGvmaVmid { vmid },
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceVvma { .. } => FenceOperation::HFenceVvma,
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceVvmaAsid { asid, .. } => {
                FenceOperation::HFenceVvmaAsid { asid }
            }
        }
    }

    /// Reports whether each executing target hart must implement H.
    pub fn requires_hypervisor(self) -> bool {
        #[cfg(feature = "hypervisor")]
        {
            matches!(
                self.operation,
                RequestOperation::HFenceGvma
                    | RequestOperation::HFenceGvmaVmid { .. }
                    | RequestOperation::HFenceVvma { .. }
                    | RequestOperation::HFenceVvmaAsid { .. }
            )
        }
        #[cfg(not(feature = "hypervisor"))]
        {
            false
        }
    }
}

impl LocalFence {
    /// Executes a checked request completely on the current hart.
    ///
    /// Policy supplies the byte-size threshold for choosing a complete flush.
    /// A larger range may invalidate extra entries.
    /// Runtime checks this hart's H extension before any hypervisor instruction,
    /// including a complete flush.
    pub fn execute(
        &self,
        request: FenceRequest,
        full_flush_limit: usize,
    ) -> Result<(), FenceError> {
        self.assert_current();
        #[cfg(feature = "hypervisor")]
        if request.requires_hypervisor()
            && !Misa::read()
                .expect("misa is required in M-mode")
                .has_extension('H')
        {
            return Err(FenceError::HypervisorUnavailable);
        }

        match request.operation {
            RequestOperation::FenceI => fence::fence_i(),
            RequestOperation::SFenceVma => {
                request.range.apply(full_flush_limit, |addr| match addr {
                    None => fence::sfence_vma_all(),
                    Some(addr) => fence::sfence_vma_addr(addr),
                })
            }
            RequestOperation::SFenceVmaAsid { asid } => {
                request.range.apply(full_flush_limit, |addr| match addr {
                    None => fence::sfence_vma_asid(asid),
                    Some(addr) => fence::sfence_vma_addr_asid(addr, asid),
                });
            }
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceGvma => {
                request.range.apply(full_flush_limit, |addr| match addr {
                    None => fence::hfence_gvma_all(),
                    Some(addr) => fence::hfence_gvma_addr(addr),
                })
            }
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceGvmaVmid { vmid } => {
                request.range.apply(full_flush_limit, |addr| match addr {
                    None => fence::hfence_gvma_vmid(vmid),
                    Some(addr) => fence::hfence_gvma_addr_vmid(addr, vmid),
                });
            }
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceVvma { guest_context } => {
                guest_context.execute(|| {
                    request.range.apply(full_flush_limit, |addr| match addr {
                        None => fence::hfence_vvma_all(),
                        Some(addr) => fence::hfence_vvma_addr(addr),
                    });
                })?;
            }
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceVvmaAsid {
                asid,
                guest_context,
            } => {
                guest_context.execute(|| {
                    request.range.apply(full_flush_limit, |addr| match addr {
                        None => fence::hfence_vvma_asid(asid),
                        Some(addr) => fence::hfence_vvma_addr_asid(addr, asid),
                    });
                })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;
    use alloc::vec::Vec;

    fn addresses(start: usize, size: usize, limit: usize) -> Vec<Option<usize>> {
        let mut addresses = Vec::new();
        AddressRange::new(start, size)
            .unwrap()
            .apply(limit, |addr| addresses.push(addr));
        addresses
    }

    #[test]
    fn special_ranges_ignore_start_and_avoid_overflow() {
        assert_eq!(addresses(0, 0, 0), vec![None]);
        assert_eq!(addresses(1, usize::MAX, usize::MAX), vec![None]);
    }

    #[test]
    fn invalid_ranges_are_rejected_before_flush() {
        assert_eq!(
            AddressRange::new(1, PAGE_SIZE),
            Err(FenceError::UnalignedAddress)
        );
        let last_page = usize::MAX & !(PAGE_SIZE - 1);
        assert_eq!(
            AddressRange::new(last_page, PAGE_SIZE),
            Err(FenceError::AddressOverflow)
        );
        assert_eq!(
            addresses(last_page, PAGE_SIZE - 1, usize::MAX),
            vec![Some(last_page)]
        );
    }

    #[test]
    fn partial_pages_and_empty_ranges_are_covered() {
        assert_eq!(addresses(PAGE_SIZE, 0, usize::MAX), vec![]);
        assert_eq!(
            addresses(PAGE_SIZE, PAGE_SIZE + 1, usize::MAX),
            vec![Some(PAGE_SIZE), Some(2 * PAGE_SIZE)]
        );
    }

    #[test]
    fn policy_threshold_selects_complete_flush() {
        assert_eq!(
            addresses(PAGE_SIZE, PAGE_SIZE, PAGE_SIZE),
            vec![Some(PAGE_SIZE)]
        );
        assert_eq!(addresses(PAGE_SIZE, PAGE_SIZE + 1, PAGE_SIZE), vec![None]);
    }
}
