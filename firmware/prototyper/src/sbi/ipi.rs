//! Inter-processor interrupts and firmware IPI delivery.
//!
//! # References
//!
//! - Specification: [RISC-V SBI IPI extension](https://docs.riscv.org/reference/sbi/v3.0/ext-ipi.html) —
//!   hart-mask handling and IPI delivery semantics.

#![forbid(unsafe_code)]

use super::pmu::pmu_firmware_counter_increment;
use crate::sbi::rfence;
use alloc::vec::Vec;
use core::sync::atomic::{
    AtomicU8,
    Ordering::{Acquire, Relaxed, Release},
};
use runtime::{
    Result,
    hart::{self, HartId, PerHart},
    ipi::{Ipi, IpiRequest},
    rustsbi::{self, HartMask, SbiRet},
};
use sbi_spec::pmu::firmware_event;

/// Pending event bit for a supervisor software interrupt.
pub(crate) const IPI_TYPE_SSOFT: u8 = 1 << 0;
/// Pending event bit for remote fence operations.
pub(crate) const IPI_TYPE_FENCE: u8 = 1 << 1;

/// IPI work posted by senders and consumed by the target hart.
// Align records to reduce false sharing between harts. The fixed alignment is
// a performance choice and does not guarantee cache-line isolation.
#[repr(align(128))]
struct PendingIpis {
    events: AtomicU8,
}

impl PendingIpis {
    const fn new() -> Self {
        Self {
            events: AtomicU8::new(0),
        }
    }

    fn post(&self, events: u8) {
        self.events.fetch_or(events, Release);
    }

    fn fence_pending(&self) -> bool {
        self.events.load(Relaxed) & IPI_TYPE_FENCE != 0
    }

    fn take(&self) -> u8 {
        self.events.swap(0, Acquire)
    }
}

/// SBI IPI extension.
///
/// The extension probes unavailable without an IPI device.
pub struct SbiIpi {
    ipi: Ipi,
    pending: PerHart<PendingIpis>,
}

impl rustsbi::Ipi for SbiIpi {
    fn _rustsbi_probe(&self) -> usize {
        usize::from(self.ipi.delivery().is_some())
    }

    /// Sends IPIs to the specified harts.
    #[inline]
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        pmu_firmware_counter_increment(firmware_event::IPI_SENT);
        let requests = match target_requests(hart_mask) {
            Ok(requests) => requests,
            Err(error) => return error,
        };

        for req in requests {
            for hart_id in req.harts() {
                let hart = HartId::from_raw(hart_id).expect("BUG: unknown hart ID");
                self.pending.get(hart).post(IPI_TYPE_SSOFT);
            }
            // Always signal: pending bits can remain after a failed send.
            if self.ipi.send(req).is_err() {
                return SbiRet::failed();
            }
        }

        SbiRet::success(0)
    }
}

impl SbiIpi {
    pub(crate) fn new(ipi: Ipi) -> Result<Self> {
        let pending = PerHart::new(|_| PendingIpis::new())?;
        Ok(Self { ipi, pending })
    }

    pub(crate) fn ipi(&self) -> &Ipi {
        &self.ipi
    }

    /// Sends an IPI carrying a remote fence operation.
    pub fn send_fence_by_ipi(&self, hart_mask: HartMask, ctx: rfence::RFenceContext) -> SbiRet {
        let current_hart =
            HartId::current().expect("BUG: current hart is not in the boot topology");
        let requests = match target_requests(hart_mask) {
            Ok(requests) => requests,
            Err(error) => return error,
        };
        let local = rfence::local_rfence_for(current_hart);
        let mut result = SbiRet::success(0);

        for hart_id in requests.flat_map(IpiRequest::harts) {
            // Handle the local request without sending an IPI.
            if hart_id == current_hart.as_usize() {
                rfence::rfence_local_handler(ctx);
                continue;
            }

            let target =
                HartId::from_raw(hart_id).expect("BUG: target hart is not in the boot topology");
            let remote = rfence::remote_rfence_for(target);
            local.add();
            remote.set(ctx);

            self.pending.get(target).post(IPI_TYPE_FENCE);
            if self.ipi.send(IpiRequest::for_hart(target)).is_ok() {
                continue;
            }
            // Cancel this source's queued request; a receiver that already
            // took it remains responsible for the acknowledgement.
            if remote.cancel(current_hart.as_usize()) {
                rfence::remote_rfence_for(current_hart).sub();
            }
            result = SbiRet::failed();
            break;
        }

        // Complete previously submitted operations even if a later send failed.
        while !local.is_sync() {
            rfence::rfence_poll_for(current_hart);
        }

        result
    }

    /// Returns whether `hart` has a pending fence event.
    pub(super) fn fence_pending(&self, hart: HartId) -> bool {
        self.pending.get(hart).fence_pending()
    }
}

/// Delivers SBI work after Runtime acknowledges the device interrupt.
///
/// Returns whether Runtime must notify the supervisor.
pub(crate) fn deliver_current() -> bool {
    // The device clear/claim must precede the pending-event read.
    crate::riscv::csr::fence::io_to_memory();
    let hart = HartId::current().expect("BUG: current hart is not in the boot topology");
    let ipi = &super::SBI_DISPATCHER
        .get()
        .expect("BUG: SBI dispatcher not initialized")
        .ipi;
    let ipi_type = ipi.pending.get(hart).take();
    let notify_supervisor = (ipi_type & IPI_TYPE_SSOFT) != 0;
    if notify_supervisor {
        pmu_firmware_counter_increment(firmware_event::IPI_RECEIVED);
    }
    if (ipi_type & IPI_TYPE_FENCE) != 0 {
        rfence::rfence_handler();
    }
    notify_supervisor
}

fn target_requests(hart_mask: HartMask) -> Result<impl Iterator<Item = IpiRequest>, SbiRet> {
    let available = |hart: HartId| {
        crate::platform::hart_privilege_checked(hart.as_usize()) && hart::can_receive_ipi(hart)
    };
    let (mask, base) = hart_mask.into_inner();
    let mut single = None;
    let mut requests: Vec<IpiRequest> = Vec::new();
    if base == usize::MAX {
        // Ignore mask and expand all available harts into ordinary windows.
        for hart in HartId::all().filter(|hart| available(*hart)) {
            let singleton = IpiRequest::for_hart(hart);
            if !requests
                .last_mut()
                .is_some_and(|request| request.try_merge(singleton))
            {
                requests.push(singleton);
            }
        }
    } else if mask != 0 {
        // A hart in the topology remains a valid target even when it is
        // stopped or has not yet passed the platform privilege check.
        let mut active_mask = 0;
        for bit in HartMask::from_mask_base(mask, 0) {
            let hart_id = base.checked_add(bit).ok_or_else(SbiRet::invalid_param)?;
            let hart = HartId::from_raw(hart_id).map_err(|_| SbiRet::invalid_param())?;
            if available(hart) {
                active_mask |= 1 << bit;
            }
        }
        // An ordinary mask needs one window, without a heap allocation.
        if active_mask != 0 {
            single = Some(
                IpiRequest::from_mask_base(active_mask, base)
                    .map_err(|_| SbiRet::invalid_param())?,
            );
        }
    }
    Ok(single.into_iter().chain(requests))
}
