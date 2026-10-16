//! Inter-processor interrupts and firmware IPI delivery.
//!
//! # References
//!
//! - Specification: [RISC-V SBI IPI extension](https://docs.riscv.org/reference/sbi/v3.0/ext-ipi.html) —
//!   hart-mask handling and IPI delivery semantics.

#![forbid(unsafe_code)]

use super::pmu::{FirmwareEvent, SbiPmu};
use alloc::vec::Vec;
use bitflags::bitflags;
use core::sync::atomic::{AtomicU8, Ordering};
use runtime::{
    Result,
    hart::{Hart, HartId, HartLocal},
    ipi::{Ipi, IpiRequest},
    rustsbi::{self, HartMask, SbiRet},
};

bitflags! {
    #[derive(Clone, Copy)]
    struct PendingIpis: u8 {
        const SUPERVISOR = 1 << 0;
        const FENCE = 1 << 1;
    }
}

/// SBI IPI extension.
/// Owns Runtime's IPI state; without a device, the extension probes unavailable.
pub(crate) struct SbiIpi {
    ipi: Ipi,
    pending: HartLocal<AtomicU8>,
}

impl rustsbi::Ipi for SbiIpi {
    fn _rustsbi_probe(&self) -> usize {
        usize::from(self.ipi.delivery().is_some())
    }

    /// Sends IPIs to the specified harts.
    #[inline]
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        SbiPmu::record_event(FirmwareEvent::IpiSent);
        let requests = match target_requests(hart_mask) {
            Ok(requests) => requests,
            Err(error) => return error,
        };

        for req in requests {
            for hart_id in req.harts() {
                let hart = HartId::from_raw(hart_id).expect("validated IPI target");
                self.pending
                    .get(hart)
                    .expect("IPI target record")
                    .fetch_or(PendingIpis::SUPERVISOR.bits(), Ordering::Release);
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
        let pending = HartLocal::new();
        pending.init_all(|_| AtomicU8::new(0))?;
        Ok(Self { ipi, pending })
    }

    pub(crate) fn ipi(&self) -> &Ipi {
        &self.ipi
    }

    /// Publishes fence work before signaling the target hart.
    pub(super) fn send_fence(&self, hart: HartId) -> Result<()> {
        self.pending
            .get(hart)?
            .fetch_or(PendingIpis::FENCE.bits(), Ordering::Release);
        self.ipi.send(IpiRequest::for_hart(hart))
    }

    pub(super) fn fence_pending(&self) -> bool {
        let hart = HartId::current().expect("current IPI hart");
        PendingIpis::from_bits_retain(
            self.pending
                .get(hart)
                .expect("IPI record")
                .load(Ordering::Relaxed),
        )
        .contains(PendingIpis::FENCE)
    }

    /// Runtime calls this after acknowledging the local device interrupt.
    pub(crate) fn deliver_current() -> bool {
        let ipi = super::ipi().expect("published IPI adapter");
        let hart = HartId::current().expect("current IPI hart");
        let events = PendingIpis::from_bits_retain(
            ipi.pending
                .get(hart)
                .expect("IPI record")
                .swap(0, Ordering::Acquire),
        );
        let notify = events.contains(PendingIpis::SUPERVISOR);
        if notify {
            SbiPmu::record_event(FirmwareEvent::IpiReceived);
        }
        if events.contains(PendingIpis::FENCE) {
            super::rfence().expect("published RFENCE adapter").handle();
        }
        notify
    }
}

pub(super) fn target_requests(
    hart_mask: HartMask,
) -> Result<impl Iterator<Item = IpiRequest> + Clone, SbiRet> {
    let available = |hart: HartId| {
        crate::platform::hart_privilege_checked(hart.as_usize())
            && Hart::get(hart).is_ok_and(Hart::can_receive_ipi)
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
        // Validate platform assignment separately from transient HSM state.
        // Stopped harts remain valid targets but need no supervisor IPI.
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
