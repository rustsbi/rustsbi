//! Remote fence operations.
//!
//! # References
//!
//! - Specification: [RISC-V SBI RFENCE extension](https://docs.riscv.org/reference/sbi/v3.0/ext-rfence.html) —
//!   hart-mask and address-range semantics for remote fences.
//! - Specification: [RISC-V hypervisor extension](https://docs.riscv.org/reference/isa/priv/hypervisor.html) —
//!   hypervisor fence instructions and their execution requirements.

#![forbid(unsafe_code)]

use runtime::{
    Error, Result,
    fence::{self, FlushRange, LocalFence},
    hart::HartId,
    ipi::{self, IpiEvent},
    rustsbi::{Fence, HartMask, SbiRet},
};
use sbi_spec::pmu::firmware_event;

use crate::cfg::{PAGE_SIZE, TLB_FLUSH_LIMIT};

pub(super) mod queue;
use super::{
    features::{self, Extension},
    ipi::{active_target_harts, validate_hart_mask},
    pmu::pmu_firmware_counter_increment,
};

/// Creates one fence queue and synchronization counter per enabled hart.
///
/// The caller must run this after Runtime publishes the hart topology and
/// before any hart enqueues or services a remote fence.
pub(crate) fn init() {
    queue::init();
}

/// Implementation of RISC-V remote fence operations.
pub(crate) struct SbiRFence;

impl SbiRFence {
    pub(crate) fn new() -> Result<Self> {
        ipi::get()
            .ok_or(Error::NotReady)?
            .register(IpiEvent::Rfence, rfence_handler)?;
        Ok(Self)
    }
}

/// Translates an SBI address range into a local flush range.
fn flush_range(start_addr: usize, size: usize) -> Result<FlushRange, SbiRet> {
    if (start_addr == 0 && size == 0) || size == usize::MAX {
        return Ok(FlushRange::All);
    }
    if start_addr & (PAGE_SIZE - 1) != 0 || (size > 0 && start_addr > usize::MAX - (size - 1)) {
        return Err(SbiRet::invalid_address());
    }
    if size > TLB_FLUSH_LIMIT {
        return Ok(FlushRange::All);
    }
    Ok(FlushRange::Pages {
        start: start_addr,
        size,
        stride: PAGE_SIZE,
    })
}

fn dispatch_validated_fence(
    op: LocalFence,
    current_hart: HartId,
    targets: impl Iterator<Item = HartId>,
) -> SbiRet {
    let current = queue::rfence_cell(current_hart);
    let local = current.local();
    let mut result = SbiRet::success(0);

    for target in targets {
        if target == current_hart {
            rfence_local_handler(op);
            continue;
        }

        let remote = queue::rfence_cell(target).remote();
        local.add();
        while !remote.try_push((op, current_hart)) {
            rfence_single_handler_for(current_hart);
        }

        if ipi::get().is_some_and(|ipi| ipi.send(IpiEvent::Rfence, [target]).is_ok()) {
            continue;
        }
        // A receiver that already took the request remains responsible for
        // its acknowledgement.
        if remote.cancel(current_hart) {
            current.remote().sub();
        }
        result = SbiRet::failed();
        break;
    }

    // Complete previously submitted operations even if a later send failed.
    while !local.is_sync() {
        rfence_single_handler_for(current_hart);
    }

    result
}

/// Validates target harts, records the sent PMU event, and executes the fence.
fn remote_fence_process(
    op: LocalFence,
    current_hart: HartId,
    hart_mask: HartMask,
    sent_event: usize,
) -> SbiRet {
    if let Err(error) = validate_hart_mask(hart_mask) {
        return error;
    }
    if let Err(error) = validate_target_harts(op, current_hart, hart_mask) {
        return error;
    }
    if matches!(
        op,
        LocalFence::Supervisor {
            range: FlushRange::Pages { size: 0, .. },
            ..
        } | LocalFence::GuestPhysical {
            range: FlushRange::Pages { size: 0, .. },
            ..
        } | LocalFence::GuestVirtual {
            range: FlushRange::Pages { size: 0, .. },
            ..
        }
    ) {
        return SbiRet::success(0);
    }
    pmu_firmware_counter_increment(sent_event);
    dispatch_validated_fence(op, current_hart, active_target_harts(hart_mask))
}

fn validate_target_harts(
    op: LocalFence,
    current_hart: HartId,
    hart_mask: HartMask,
) -> Result<(), SbiRet> {
    let mut has_active_target = false;
    for target in active_target_harts(hart_mask) {
        has_active_target = true;
        match op {
            LocalFence::Instruction => {}
            LocalFence::Supervisor { asid, .. } => {
                if let Some(asid) = asid
                    && !asid_valid_for_hart(asid, target)
                {
                    return Err(SbiRet::invalid_param());
                }
            }
            LocalFence::GuestPhysical { vmid, .. } => {
                if !hart_supports_hypervisor(target) {
                    return Err(SbiRet::not_supported());
                }
                if let Some(vmid) = vmid
                    && !vmid_valid_for_hart(vmid, target)
                {
                    return Err(SbiRet::invalid_param());
                }
            }
            LocalFence::GuestVirtual { asid, vmid, .. } => {
                if !hart_supports_hypervisor(target) {
                    return Err(SbiRet::not_supported());
                }
                if let Some(asid) = asid
                    && !asid_valid_for_hart(asid, target)
                {
                    return Err(SbiRet::invalid_param());
                }
                if !vmid_valid_for_hart(vmid, target) {
                    return Err(SbiRet::not_supported());
                }
            }
        }
    }

    if !has_active_target {
        match op {
            LocalFence::Supervisor {
                asid: Some(asid), ..
            }
            | LocalFence::GuestVirtual {
                asid: Some(asid), ..
            } if !asid_valid_for_hart(asid, current_hart) => {
                return Err(SbiRet::invalid_param());
            }
            LocalFence::GuestPhysical {
                vmid: Some(vmid), ..
            } if !vmid_valid_for_hart(vmid, current_hart) => {
                return Err(SbiRet::invalid_param());
            }
            _ => {}
        }
    }
    Ok(())
}

fn asid_valid_for_hart(asid: usize, hart: HartId) -> bool {
    features::supervisor_xlen(hart).map_or_else(
        || fence::is_valid_asid(asid),
        |xlen| fence::is_valid_asid_for(asid, xlen),
    )
}

fn vmid_valid_for_hart(vmid: usize, hart: HartId) -> bool {
    features::supervisor_xlen(hart).map_or_else(
        || fence::is_valid_vmid(vmid),
        |xlen| fence::is_valid_vmid_for(vmid, xlen),
    )
}

fn hart_supports_hypervisor(hart: HartId) -> bool {
    features::has_extension(hart, Extension::Hypervisor)
}

fn current_hart_with_hypervisor() -> Result<HartId, SbiRet> {
    let current_hart = HartId::current().map_err(SbiRet::from)?;
    if !hart_supports_hypervisor(current_hart) {
        return Err(SbiRet::not_supported());
    }
    Ok(current_hart)
}

fn remote_sfence_vma_op(
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
    asid: Option<usize>,
    sent_event: usize,
) -> Result<SbiRet, SbiRet> {
    let current_hart = HartId::current().map_err(SbiRet::from)?;
    let range = flush_range(start_addr, size)?;
    Ok(remote_fence_process(
        LocalFence::Supervisor { range, asid },
        current_hart,
        hart_mask,
        sent_event,
    ))
}

fn remote_hfence_gvma_op(
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
    vmid: Option<usize>,
    sent_event: usize,
) -> Result<SbiRet, SbiRet> {
    let current_hart = HartId::current().map_err(SbiRet::from)?;
    let range = flush_range(start_addr, size)?;
    Ok(remote_fence_process(
        LocalFence::GuestPhysical { range, vmid },
        current_hart,
        hart_mask,
        sent_event,
    ))
}

fn remote_hfence_vvma_op(
    hart_mask: HartMask,
    start_addr: usize,
    size: usize,
    asid: Option<usize>,
    sent_event: usize,
) -> Result<SbiRet, SbiRet> {
    let current_hart = current_hart_with_hypervisor()?;
    let range = flush_range(start_addr, size)?;
    let vmid = fence::current_guest_vmid().map_err(|_| SbiRet::not_supported())?;
    Ok(remote_fence_process(
        LocalFence::GuestVirtual { range, asid, vmid },
        current_hart,
        hart_mask,
        sent_event,
    ))
}

impl Fence for SbiRFence {
    fn remote_fence_i(&self, hart_mask: HartMask) -> SbiRet {
        let current_hart = match HartId::current() {
            Ok(hart) => hart,
            Err(error) => return error.into(),
        };
        remote_fence_process(
            LocalFence::Instruction,
            current_hart,
            hart_mask,
            firmware_event::FENCE_I_SENT,
        )
    }

    fn remote_sfence_vma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        remote_sfence_vma_op(
            hart_mask,
            start_addr,
            size,
            None,
            firmware_event::SFENCE_VMA_SENT,
        )
        .unwrap_or_else(|err| err)
    }

    fn remote_sfence_vma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        remote_sfence_vma_op(
            hart_mask,
            start_addr,
            size,
            Some(asid),
            firmware_event::SFENCE_VMA_ASID_SENT,
        )
        .unwrap_or_else(|err| err)
    }

    fn remote_hfence_gvma_vmid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        vmid: usize,
    ) -> SbiRet {
        remote_hfence_gvma_op(
            hart_mask,
            start_addr,
            size,
            Some(vmid),
            firmware_event::HFENCE_GVMA_VMID_SENT,
        )
        .unwrap_or_else(|err| err)
    }

    fn remote_hfence_gvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        remote_hfence_gvma_op(
            hart_mask,
            start_addr,
            size,
            None,
            firmware_event::HFENCE_GVMA_SENT,
        )
        .unwrap_or_else(|err| err)
    }

    fn remote_hfence_vvma_asid(
        &self,
        hart_mask: HartMask,
        start_addr: usize,
        size: usize,
        asid: usize,
    ) -> SbiRet {
        remote_hfence_vvma_op(
            hart_mask,
            start_addr,
            size,
            Some(asid),
            firmware_event::HFENCE_VVMA_ASID_SENT,
        )
        .unwrap_or_else(|err| err)
    }

    fn remote_hfence_vvma(&self, hart_mask: HartMask, start_addr: usize, size: usize) -> SbiRet {
        remote_hfence_vvma_op(
            hart_mask,
            start_addr,
            size,
            None,
            firmware_event::HFENCE_VVMA_SENT,
        )
        .unwrap_or_else(|err| err)
    }
}

/// Handles one fence operation queued for `hart`, returning whether one was dequeued.
#[inline]
fn rfence_single_handler_for(hart: HartId) -> bool {
    if let Some((op, source_hart)) = queue::rfence_cell(hart).local().get() {
        rfence_local_handler(op);
        queue::rfence_cell(source_hart).remote().sub();
        true
    } else {
        false
    }
}

/// Executes a fence on this hart and records its received PMU event.
#[inline]
fn rfence_local_handler(op: LocalFence) {
    if fence::execute_local(op).is_err() {
        return;
    }
    let event = match op {
        LocalFence::Instruction => firmware_event::FENCE_I_RECEIVED,
        LocalFence::Supervisor { asid: None, .. } => firmware_event::SFENCE_VMA_RECEIVED,
        LocalFence::Supervisor { asid: Some(_), .. } => firmware_event::SFENCE_VMA_ASID_RECEIVED,
        LocalFence::GuestPhysical { vmid: None, .. } => firmware_event::HFENCE_GVMA_RECEIVED,
        LocalFence::GuestPhysical { vmid: Some(_), .. } => {
            firmware_event::HFENCE_GVMA_VMID_RECEIVED
        }
        LocalFence::GuestVirtual { asid: None, .. } => firmware_event::HFENCE_VVMA_RECEIVED,
        LocalFence::GuestVirtual { asid: Some(_), .. } => firmware_event::HFENCE_VVMA_ASID_RECEIVED,
    };
    pmu_firmware_counter_increment(event);
}

/// Processes all pending remote fence operations on the current hart.
#[inline]
fn rfence_handler(hart: HartId) {
    while rfence_single_handler_for(hart) {}
}
