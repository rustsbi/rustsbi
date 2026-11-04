//! Inter-processor interrupts and firmware IPI delivery.
//!
//! # References
//!
//! - Specification: [RISC-V SBI IPI extension](https://docs.riscv.org/reference/sbi/v3.0/ext-ipi.html) —
//!   hart-mask handling and IPI delivery semantics.

#![forbid(unsafe_code)]

use super::pmu::pmu_firmware_counter_increment;
use crate::{platform::hart_privilege_checked, sbi::rfence};
use core::sync::atomic::{
    AtomicU8,
    Ordering::{Acquire, Release},
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

    fn take(&self) -> u8 {
        self.events.swap(0, Acquire)
    }
}

/// SBI IPI extension.
///
/// The extension probes unavailable without an IPI device.
pub struct SbiIpi {
    ipi: &'static Ipi,
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
        let targets = match target_harts(hart_mask) {
            Ok(targets) => targets,
            Err(error) => return error,
        };

        for hart in targets {
            self.pending.get(hart).post(IPI_TYPE_SSOFT);
            // Always signal: pending bits can remain after a failed send.
            if self.ipi.send(IpiRequest::for_hart(hart)).is_err() {
                return SbiRet::failed();
            }
        }

        SbiRet::success(0)
    }
}

impl SbiIpi {
    pub(crate) fn new(ipi: &'static Ipi) -> Result<Self> {
        let pending = PerHart::new(|_| PendingIpis::new())?;
        Ok(Self { ipi, pending })
    }

    pub(crate) fn ipi(&self) -> &'static Ipi {
        self.ipi
    }

    pub(super) fn send_fence(&self, hart: HartId) -> Result<()> {
        self.pending.get(hart).post(IPI_TYPE_FENCE);
        self.ipi.send(IpiRequest::for_hart(hart))
    }
}

/// Delivers SBI work after Runtime acknowledges the device interrupt.
///
/// Returns whether Runtime must notify the supervisor.
pub(crate) fn deliver_current() -> bool {
    // The device clear/claim must precede the pending-event read.
    runtime::instruction::fence();
    let hart = HartId::current().expect("BUG: current hart is not in the boot topology");
    let ipi = &super::SbiDispatcher::get()
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

/// Validates the mask before yielding any target.
///
/// Stopped harts remain valid but are skipped.
/// Availability is checked as each hart is visited and can change concurrently
/// with hart lifecycle calls.
pub(super) fn target_harts(hart_mask: HartMask) -> Result<impl Iterator<Item = HartId>, SbiRet> {
    let (mask, base) = hart_mask.into_inner();
    if base != HartMask::IGNORE_MASK {
        // Reject an invalid mask before sending to any target.
        for bit in HartMask::from_mask_base(mask, 0) {
            let raw = base.checked_add(bit).ok_or_else(SbiRet::invalid_param)?;
            HartId::from_raw(raw).map_err(|_| SbiRet::invalid_param())?;
        }
    }

    Ok(HartId::all().filter(move |hart| {
        hart_mask.has_bit(hart.as_usize())
            && hart_privilege_checked(hart.as_usize())
            && hart::can_receive_ipi(*hart)
    }))
}
