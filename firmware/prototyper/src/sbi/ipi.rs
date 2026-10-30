//! Inter-processor interrupts and firmware IPI delivery.
//!
//! # References
//!
//! - Specification: [RISC-V SBI IPI extension](https://docs.riscv.org/reference/sbi/v3.0/ext-ipi.html) —
//!   hart-mask handling and IPI delivery semantics.

#![forbid(unsafe_code)]

mod pending;

use super::pmu::{FirmwareEvent, SbiPmu};
use alloc::vec::Vec;
use pending::{IpiEvent, PendingIpis};
use runtime::{
    Error, Result,
    hart::{Hart, HartId, HartLocal},
    ipi::{Ipi, IpiRequest},
    rustsbi::{self, HartMask, SbiRet},
};

/// SBI IPI extension.
/// Uses Runtime's IPI device; owns only SBI pending-event records.
pub struct SbiIpi {
    ipi: &'static Ipi,
    pending: HartLocal<PendingIpis>,
}

impl rustsbi::Ipi for SbiIpi {
    fn _rustsbi_probe(&self) -> usize {
        usize::from(self.ipi.delivery().is_some())
    }

    /// Sends IPIs to the specified harts.
    #[inline]
    fn send_ipi(&self, hart_mask: HartMask) -> SbiRet {
        SbiPmu::record_event(FirmwareEvent::IpiSent);
        let requests = match resolve_ipi_targets(hart_mask) {
            Ok(requests) => requests,
            Err(error) => return error,
        };

        for request in requests {
            if self.send_supervisor_ipi(request).is_err() {
                return SbiRet::failed();
            }
        }

        SbiRet::success(0)
    }
}

impl SbiIpi {
    pub(crate) fn new(ipi: &'static Ipi) -> Result<Self> {
        let pending = HartLocal::new();
        pending.init_all(|_| PendingIpis::new())?;
        ipi.register_pending(Self::handle_pending_ipis)?;
        Ok(Self { ipi, pending })
    }

    pub(crate) fn is_available(&self) -> bool {
        self.ipi.delivery().is_some()
    }

    /// Records supervisor notifications before signaling their target window.
    fn send_supervisor_ipi(&self, request: IpiRequest) -> Result<()> {
        for hart in request.harts() {
            self.pending.get(hart)?.insert(IpiEvent::Supervisor);
        }
        // Always signal: pending events can remain after a failed send.
        self.ipi.send(request)
    }

    /// Publishes fence work before signaling the target hart.
    pub(super) fn send_fence(&self, target_hart: HartId) -> Result<()> {
        self.pending.get(target_hart)?.insert(IpiEvent::Fence);
        self.ipi.send(IpiRequest::for_hart(target_hart))
    }

    /// Handles all firmware IPI work after Runtime acknowledges the device.
    ///
    /// Returns whether a supervisor software interrupt must be raised.
    pub(crate) fn handle_pending_ipis(hart: HartId) -> Result<bool> {
        let dispatcher = super::SBI_DISPATCHER.get().ok_or(Error::NotReady)?;
        let pending = dispatcher.ipi.pending.get(hart)?;
        let mut notified = false;
        while let Some(event) = pending.pop_pending() {
            match event {
                IpiEvent::Supervisor => {
                    if !notified {
                        SbiPmu::record_event(FirmwareEvent::IpiReceived);
                    }
                    notified = true;
                }
                IpiEvent::Fence => {
                    dispatcher
                        .rfence
                        .as_ref()
                        .ok_or(Error::NotReady)?
                        .handle(hart);
                }
            }
        }
        Ok(notified)
    }
}

/// Resolves an SBI hart mask into validated IPI target windows.
///
/// An ordinary mask produces at most one active window in a stack `Option`, so
/// the common path does not allocate. Broadcast ignores the mask and expands
/// all available harts into potentially several windows in a `Vec`.
pub(super) fn resolve_ipi_targets(
    hart_mask: HartMask,
) -> Result<impl Iterator<Item = IpiRequest> + Clone, SbiRet> {
    let (mask, base) = hart_mask.into_inner();
    let (mask_window, broadcast_windows) = if base == usize::MAX {
        (None, broadcast_windows())
    } else {
        (ordinary_mask_window(mask, base)?, Vec::new())
    };
    Ok(mask_window.into_iter().chain(broadcast_windows))
}

/// Validates an ordinary mask and retains only harts that accept IPIs now.
fn ordinary_mask_window(mask: usize, base: usize) -> Result<Option<IpiRequest>, SbiRet> {
    let mut active_mask = 0;
    for bit in HartMask::from_mask_base(mask, 0) {
        let hart_id = base.checked_add(bit).ok_or_else(SbiRet::invalid_param)?;
        let hart = HartId::from_raw(hart_id).map_err(|_| SbiRet::invalid_param())?;
        // HartId validates topology membership. Harts outside supervisor
        // execution, including stopped harts, need no supervisor IPI.
        if ipi_target_available(hart) {
            active_mask |= 1 << bit;
        }
    }
    if active_mask == 0 {
        return Ok(None);
    }
    IpiRequest::from_mask_base(active_mask, base)
        .map(Some)
        .map_err(|_| SbiRet::invalid_param())
}

/// Expands the broadcast form into merged windows for all available harts.
fn broadcast_windows() -> Vec<IpiRequest> {
    let mut windows: Vec<IpiRequest> = Vec::new();
    for hart in HartId::all().filter(|hart| ipi_target_available(*hart)) {
        let singleton = IpiRequest::for_hart(hart);
        if !windows
            .last_mut()
            .is_some_and(|window| window.try_merge(singleton))
        {
            windows.push(singleton);
        }
    }
    windows
}

fn ipi_target_available(hart: HartId) -> bool {
    crate::platform::hart_privilege_checked(hart.as_usize())
        && Hart::get(hart).is_ok_and(Hart::can_receive_ipi)
}
