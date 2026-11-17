//! Checked instruction-fetch and supervisor-translation fences on this hart.
//!
//! Firmware policy selects target harts and transports requests. Runtime owns
//! request validation and local execution. Hypervisor virtual-supervisor
//! requests carry the calling hart's VMID and restore the target's state before
//! returning.
//!
//! # Supported widths
//!
//! Translation requests require SXLEN == MXLEN on the requesting hart and on
//! each executing target. VVMA also requires VSXLEN == MXLEN. RV64 firmware
//! rejects narrower supervisor/guest contexts with `NotSupported` rather than
//! decoding their CSRs with RV64 layouts. RV32 has no SXL/VSXL fields.
//! Execution of an empty range does not need a target translation context.
//!
//! # Execution invariant
//!
//! Controlled Runtime boot and trap entry run this code in M-mode on a valid
//! Runtime stack, with mscratch=0 and MPRV=0. Readiness/hart checks below verify
//! the installed trap entry and hart binding; they do not establish the entire
//! boot protocol by themselves. No safe Runtime API enables guest accesses or
//! changes that environment during a local execution scope.

#[cfg(feature = "hypervisor")]
mod guest;
mod instructions;
mod xlen;

use crate::{Error, Result, hart::HartId};
use core::marker::PhantomData;

const PAGE_SIZE: usize = 4096;

/// The local synchronization operation to perform.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FenceOperation {
    /// Synchronizes instruction fetch with prior writes on this hart.
    FenceI,
    /// Synchronizes supervisor translations for all address-space identifiers.
    SFenceVma,
    /// Synchronizes supervisor translations for one address-space identifier.
    SFenceVmaAsid {
        /// Runtime clears reserved high bits; hardware ignores unimplemented ASID bits.
        asid: usize,
    },
    /// Synchronizes guest-physical translations for all virtual machines.
    #[cfg(feature = "hypervisor")]
    HFenceGvma,
    /// Synchronizes guest-physical translations for one virtual machine.
    #[cfg(feature = "hypervisor")]
    HFenceGvmaVmid {
        /// Runtime clears reserved high bits; hardware ignores unimplemented VMID bits.
        vmid: usize,
    },
    /// Synchronizes guest-virtual translations for all address spaces in the
    /// requesting hart's virtual machine.
    #[cfg(feature = "hypervisor")]
    HFenceVvma,
    /// Synchronizes guest-virtual translations for one address space in the
    /// requesting hart's virtual machine.
    #[cfg(feature = "hypervisor")]
    HFenceVvmaAsid {
        /// Guest address-space identifier.
        asid: usize,
    },
}

impl FenceOperation {
    /// Returns whether executing this operation requires the H extension.
    pub const fn requires_hypervisor(self) -> bool {
        match self {
            Self::FenceI | Self::SFenceVma | Self::SFenceVmaAsid { .. } => false,
            #[cfg(feature = "hypervisor")]
            Self::HFenceGvma
            | Self::HFenceGvmaVmid { .. }
            | Self::HFenceVvma
            | Self::HFenceVvmaAsid { .. } => true,
        }
    }
}

/// # Safety-usable invariant
///
/// Page ranges come from `new`: the start is page aligned and start + size
/// does not overflow. Keep these conditions when constructing a range here.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AddressRange {
    All,
    Pages { start: usize, size: usize },
}

impl AddressRange {
    fn new(start: usize, size: usize) -> Result<Self> {
        // SBI's whole-address-space encodings ignore alignment and overflow.
        if (start == 0 && size == 0) || size == usize::MAX {
            return Ok(Self::All);
        }
        if !start.is_multiple_of(PAGE_SIZE) {
            return Err(Error::InvalidArgs);
        }
        start.checked_add(size).ok_or(Error::Overflow)?;
        Ok(Self::Pages { start, size })
    }

    fn is_empty(self) -> bool {
        matches!(self, Self::Pages { size: 0, .. })
    }

    /// # Safety-usable invariant
    ///
    /// Visits only checked integer page addresses or the whole-space marker.
    /// Apart from invoking `invalidate`, this does not access guest memory,
    /// mutate machine state, reenter, or unwind. Callback effects and errors
    /// are passed through; callers must separately constrain the callback.
    fn apply(
        self,
        full_flush_limit: usize,
        mut invalidate: impl FnMut(Option<usize>) -> Result<()>,
    ) -> Result<()> {
        match self {
            Self::All => invalidate(None),
            Self::Pages { size, .. } if size > full_flush_limit => invalidate(None),
            Self::Pages { start, size } => {
                // Construction checked the exclusive end, including partial pages.
                for offset in (0..size).step_by(PAGE_SIZE) {
                    invalidate(Some(start + offset))?;
                }
                Ok(())
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
        guest: guest::GuestContext,
    },
    #[cfg(feature = "hypervisor")]
    HFenceVvmaAsid {
        asid: usize,
        guest: guest::GuestContext,
    },
}

/// An immutable, checked request that may be copied to another hart's queue.
///
/// It grants no access to page-table memory and makes no claim about the
/// executing hart's instruction support. Execution checks its own environment.
/// Only [`LocalFence::request`] can construct a request.
///
/// ```compile_fail,E0616
/// # #![no_std]
/// # #![no_main]
/// # #[panic_handler]
/// # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
/// use rustsbi_firmware_runtime::rfence::{FenceOperation, FenceRequest};
/// fn tamper(request: &mut FenceRequest) {
///     request.operation = FenceOperation::FenceI;
/// }
/// ```
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FenceRequest {
    operation: RequestOperation,
    range: AddressRange,
}

impl FenceRequest {
    fn new(operation: FenceOperation, start: usize, size: usize) -> Result<Self> {
        let range = match operation {
            FenceOperation::FenceI => AddressRange::All,
            _ => AddressRange::new(start, size)?,
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
                guest: guest::GuestContext::capture()?,
            },
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceVvmaAsid { asid } => RequestOperation::HFenceVvmaAsid {
                asid,
                guest: guest::GuestContext::capture()?,
            },
        };
        Ok(Self { operation, range })
    }

    /// Executes inside LocalFence's checked hart and interrupt-exclusion scope.
    fn execute_local(
        self,
        irq: &crate::irq::LocalIrqDisabled,
        full_flush_limit: usize,
    ) -> Result<()> {
        // Empty translation ranges must not borrow a VMID or touch guest CSRs.
        // FenceI always carries All, including when its ignored size is zero.
        if self.range.is_empty() {
            return Ok(());
        }
        xlen::check(self.operation())?;
        match self.operation {
            RequestOperation::FenceI => instructions::fence_i(irq),
            RequestOperation::SFenceVma => self.range.apply(full_flush_limit, |address| {
                instructions::sfence_vma(irq, address, None)
            }),
            RequestOperation::SFenceVmaAsid { asid } => {
                self.range.apply(full_flush_limit, |address| {
                    instructions::sfence_vma(irq, address, Some(asid))
                })
            }
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceGvma => self.range.apply(full_flush_limit, |address| {
                instructions::hfence_gvma(irq, address, None)
            }),
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceGvmaVmid { vmid } => {
                self.range.apply(full_flush_limit, |address| {
                    instructions::hfence_gvma(irq, address, Some(vmid))
                })
            }
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceVvma { guest } => {
                // SAFETY: This module's execution invariant supplies M-mode,
                // Runtime stack/recovery and clear mscratch/MPRV. The checked
                // hart binding and live IRQ scope hold through restoration.
                // xlen::check established native HS/VS widths in this scope;
                // neither the context switch nor the callback changes them.
                // This local closure only iterates a checked integer range and
                // executes recoverable VVMA; no guest access, CSR write,
                // interrupt enable, policy callback, reentry or unwind occurs.
                unsafe {
                    guest.with(irq, || {
                        self.range.apply(full_flush_limit, |address| {
                            instructions::hfence_vvma(irq, address, None)
                        })
                    })
                }
            }
            #[cfg(feature = "hypervisor")]
            RequestOperation::HFenceVvmaAsid { asid, guest } => {
                // SAFETY: The same checked execution and IRQ scope applies.
                // xlen::check established native HS/VS widths, which neither
                // the context switch nor this callback changes.
                // This local closure only executes VVMA with an integer ASID
                // and checked range; it has none of GuestContext::with's
                // forbidden guest access, CSR, reentry or unwind effects.
                unsafe {
                    guest.with(irq, || {
                        self.range.apply(full_flush_limit, |address| {
                            instructions::hfence_vvma(irq, address, Some(asid))
                        })
                    })
                }
            }
        }
    }

    /// Returns the requested operation, including its address-space identifier.
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

    /// Returns whether every executing target hart must implement H.
    pub fn requires_hypervisor(self) -> bool {
        self.operation().requires_hypervisor()
    }
}

/// A local executor bound to one published hart for the firmware lifetime.
///
/// Fences may be repeated and do not reserve an exclusive device. No service
/// registration is needed. The hart topology remains fixed across HSM restarts.
/// Both preparation and execution recheck the hart binding; the executor
/// cannot be transferred or shared across Rust threads.
///
/// ```compile_fail,E0277
/// # #![no_std]
/// # #![no_main]
/// # #[panic_handler]
/// # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
/// use rustsbi_firmware_runtime::rfence::LocalFence;
/// fn require_send<T: Send>() {}
/// fn transfer() { require_send::<LocalFence>(); }
/// ```
///
/// ```compile_fail,E0277
/// # #![no_std]
/// # #![no_main]
/// # #[panic_handler]
/// # fn panic(_: &core::panic::PanicInfo<'_>) -> ! { loop {} }
/// use rustsbi_firmware_runtime::rfence::LocalFence;
/// fn require_sync<T: Sync>() {}
/// fn share() { require_sync::<LocalFence>(); }
/// ```
pub struct LocalFence {
    hart: usize,
    _local: PhantomData<*mut ()>,
}

impl LocalFence {
    /// Binds an executor after this hart's Runtime trap entry is installed.
    /// Returns [`Error::NotReady`] before initialization and validates `mhartid`
    /// against the published topology.
    pub fn current() -> Result<Self> {
        if !crate::boot::traps_ready() {
            return Err(Error::NotReady);
        }
        Ok(Self {
            hart: HartId::current()?.as_usize(),
            _local: PhantomData,
        })
    }

    fn check_hart(&self, current: usize) -> Result<()> {
        if self.hart == current {
            Ok(())
        } else {
            Err(Error::AccessDenied)
        }
    }

    fn check_current(&self) -> Result<()> {
        if !crate::boot::traps_ready() {
            return Err(Error::NotReady);
        }
        self.check_hart(HartId::current()?.as_usize())
    }

    /// Validates a request without executing a fence or accessing its addresses.
    ///
    /// Translation ranges use 4 KiB base pages and a byte size, including a
    /// final partial page. `(0, 0)` and `size == usize::MAX` mean all addresses;
    /// the latter ignores `start`. Other unaligned starts return
    /// [`Error::InvalidArgs`], and a wrapping end returns [`Error::Overflow`].
    /// `FenceI` ignores both address arguments. A wrong hart returns
    /// [`Error::AccessDenied`] before constructing the request.
    /// Translation requests require SXLEN == MXLEN; VVMA additionally requires
    /// VSXLEN == MXLEN. Mixed widths return [`Error::NotSupported`] before
    /// decoding guest state. With `hypervisor`, VVMA requests capture this
    /// hart's VMID; callers cannot supply or forge that snapshot.
    pub fn request(
        &self,
        operation: FenceOperation,
        start: usize,
        size: usize,
    ) -> Result<FenceRequest> {
        self.check_current()?;
        crate::irq::with_local_disabled(|_| {
            xlen::check(operation)?;
            FenceRequest::new(operation, start, size)
        })
    }

    /// Completes a checked request on this executor's hart.
    ///
    /// Policy supplies a byte threshold: larger ranges use a complete flush.
    /// Extra invalidations are permitted, including global entries for an
    /// all-ASID flush. ASID zero uses a register operand, not architectural x0.
    /// An empty non-special range performs no invalidations or guest-context
    /// switches, after checking the hart binding and H availability.
    /// Nonempty translation requests recheck SXLEN == MXLEN on the target;
    /// VVMA also rechecks VSXLEN == MXLEN before accessing guest state.
    /// Mixed widths return [`Error::NotSupported`].
    ///
    /// Local interrupts are excluded and restored internally. No policy
    /// callbacks run inside that scope. Unsupported instructions return
    /// [`Error::NotSupported`]; prior invalidations, if any, are not rolled back.
    /// Completion synchronizes only this hart; remote completion remains the
    /// transport's responsibility.
    /// Hypervisor fences check H on the executing hart. VVMA temporarily
    /// selects the requesting hart's VMID using the target's translation
    /// configuration and restores `hgatp` and `vsatp` before returning,
    /// including on failure. A failed restoration
    /// stops the hart rather than resuming with a borrowed guest context.
    pub fn execute(&self, request: FenceRequest, full_flush_limit: usize) -> Result<()> {
        self.check_current()?;
        #[cfg(feature = "hypervisor")]
        if request.requires_hypervisor() && !crate::hart::has_hypervisor() {
            return Err(Error::NotSupported);
        }
        crate::irq::with_local_disabled(|irq| request.execute_local(irq, full_flush_limit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::{vec, vec::Vec};

    fn addresses(start: usize, size: usize, limit: usize) -> Vec<Option<usize>> {
        let mut seen = Vec::new();
        AddressRange::new(start, size)
            .unwrap()
            .apply(limit, |address| {
                seen.push(address);
                Ok(())
            })
            .unwrap();
        seen
    }

    #[test]
    fn whole_space_encodings_ignore_alignment_and_overflow() {
        assert_eq!(addresses(0, 0, 0), vec![None]);
        for start in [0, 1, PAGE_SIZE, usize::MAX] {
            assert_eq!(addresses(start, usize::MAX, usize::MAX), vec![None]);
        }
    }

    #[test]
    fn invalid_ranges_are_rejected_before_execution() {
        assert_eq!(AddressRange::new(1, 0), Err(Error::InvalidArgs));
        assert_eq!(AddressRange::new(1, PAGE_SIZE), Err(Error::InvalidArgs));
        let last_page = usize::MAX & !(PAGE_SIZE - 1);
        assert_eq!(
            AddressRange::new(last_page, PAGE_SIZE),
            Err(Error::Overflow)
        );
        assert_eq!(
            addresses(last_page, PAGE_SIZE - 1, usize::MAX),
            vec![Some(last_page)]
        );
    }

    #[test]
    fn ranges_cover_partial_pages_without_rounding_past_the_end() {
        assert_eq!(addresses(PAGE_SIZE, 0, 0), vec![]);
        assert_eq!(addresses(0, 1, usize::MAX), vec![Some(0)]);
        assert_eq!(
            addresses(PAGE_SIZE, PAGE_SIZE, usize::MAX),
            vec![Some(PAGE_SIZE)]
        );
        assert_eq!(
            addresses(PAGE_SIZE, PAGE_SIZE + 1, usize::MAX),
            vec![Some(PAGE_SIZE), Some(2 * PAGE_SIZE)]
        );
    }

    #[test]
    fn empty_range_does_not_include_whole_space_or_fence_i() {
        assert!(AddressRange::new(PAGE_SIZE, 0).unwrap().is_empty());
        assert!(!AddressRange::new(0, 0).unwrap().is_empty());
        assert!(!AddressRange::new(PAGE_SIZE, usize::MAX).unwrap().is_empty());
        assert!(
            !FenceRequest::new(FenceOperation::FenceI, PAGE_SIZE, 0)
                .unwrap()
                .range
                .is_empty()
        );
    }

    #[test]
    fn policy_threshold_can_only_broaden_the_flush() {
        assert_eq!(
            addresses(PAGE_SIZE, PAGE_SIZE, PAGE_SIZE),
            vec![Some(PAGE_SIZE)]
        );
        assert_eq!(addresses(PAGE_SIZE, PAGE_SIZE + 1, PAGE_SIZE), vec![None]);
        assert_eq!(addresses(PAGE_SIZE, 1, 0), vec![None]);
    }

    #[test]
    fn failure_stops_the_remaining_invalidations() {
        let mut seen = Vec::new();
        assert_eq!(
            AddressRange::new(0, 3 * PAGE_SIZE)
                .unwrap()
                .apply(usize::MAX, |address| {
                    seen.push(address);
                    if address == Some(PAGE_SIZE) {
                        Err(Error::NotSupported)
                    } else {
                        Ok(())
                    }
                }),
            Err(Error::NotSupported)
        );
        assert_eq!(seen, vec![Some(0), Some(PAGE_SIZE)]);
    }

    #[test]
    fn requests_preserve_asids_and_instruction_fences_ignore_ranges() {
        for asid in [0, 1, usize::MAX] {
            let operation = FenceOperation::SFenceVmaAsid { asid };
            assert_eq!(
                FenceRequest::new(operation, 0, 0).unwrap().operation(),
                operation
            );
        }
        let request = FenceRequest::new(FenceOperation::FenceI, 1, usize::MAX).unwrap();
        assert_eq!(request.range, AddressRange::All);
        fn transferable<T: Copy + Send + Sync>() {}
        transferable::<FenceRequest>();
    }

    #[cfg(feature = "hypervisor")]
    #[test]
    fn hypervisor_requests_are_marked_for_capability_preflight() {
        for operation in [
            FenceOperation::HFenceGvma,
            FenceOperation::HFenceGvmaVmid { vmid: usize::MAX },
        ] {
            let request = FenceRequest::new(operation, 0, 0).unwrap();
            assert!(request.requires_hypervisor());
            assert_eq!(request.operation(), operation);
        }
    }

    #[test]
    fn wrong_hart_is_rejected_by_the_executor_binding() {
        let local = LocalFence {
            hart: 7,
            _local: PhantomData,
        };
        assert_eq!(local.check_hart(7), Ok(()));
        assert_eq!(local.check_hart(0), Err(Error::AccessDenied));
    }
}
