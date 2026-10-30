//! IPI publication, acknowledgement, and local interrupt delivery.

use super::{Iid, Ipi, IpiDelivery, IpiRequest, imsic};
use crate::{Error, Result, irq::LocalIrqDisabled};
use core::num::NonZeroU16;
use csr::{aia::machine::Mtopei, machine};

impl Ipi {
    /// Publishes pending memory writes before sending one target window.
    pub fn send(&self, request: IpiRequest) -> Result<()> {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let device = self.device.as_deref().ok_or(Error::IoError)?;
            // SAFETY: Order published hart work before device notification.
            unsafe {
                core::arch::asm!("fence w, o", options(nostack));
            }
            device.send(request)
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = request;
            unimplemented!("IPI delivery requires RISC-V")
        }
    }

    pub(crate) fn init_current(&self, irq: &LocalIrqDisabled) -> Result<()> {
        if !crate::boot::traps_ready() {
            return Err(Error::NotReady);
        }
        if let Some(IpiDelivery::Imsic { iid, num_ids }) = self.delivery() {
            imsic::initialize(num_ids, iid, irq)?;
        }
        self.clear_current(irq)
    }

    pub(crate) fn clear_current(&self, _irq: &LocalIrqDisabled) -> Result<()> {
        match self.device.as_deref() {
            Some(device) => match device.delivery() {
                IpiDelivery::Software => device.acknowledge().map(|_| ()),
                IpiDelivery::Imsic { iid, .. } => {
                    self.claim_imsic(iid);
                    Ok(())
                }
            },
            None => Ok(()),
        }
    }

    fn claim_imsic(&self, iid: Iid) -> bool {
        // SAFETY: The selected IMSIC device establishes Mtopei support on this
        // hart. The instruction atomically claims the reported interrupt;
        // callers serialize the surrounding firmware IPI delivery sequence.
        unsafe { Mtopei::claim() }.iid().map(NonZeroU16::get) == Some(iid.number())
    }

    pub(crate) fn claim_external(&self, _irq: &LocalIrqDisabled) -> bool {
        match self.delivery() {
            Some(IpiDelivery::Imsic { iid, .. }) => self.claim_imsic(iid),
            _ => false,
        }
    }

    /// Enables hart wake sources without changing MIE or timer delivery.
    pub(crate) fn prepare_wakeup(&self, _irq: &LocalIrqDisabled) {
        // SAFETY: Boot installed Runtime entry before configuring wake sources.
        // The caller's IRQ context excludes interrupt reentry during this update.
        unsafe {
            machine::InterruptEnable::modify(|_, value| {
                value.write_field::<machine::MSIE>(machine::Interrupt::Enabled);
                if matches!(self.delivery(), Some(IpiDelivery::Imsic { .. })) {
                    value.write_field::<machine::MEIE>(machine::Interrupt::Enabled);
                }
            });
        }
    }

    pub(crate) fn handle_current(&self, _irq: &LocalIrqDisabled) -> bool {
        // Device acknowledgement must precede reads of published SBI work.
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: Orders this hart's device acknowledgement before memory reads.
        unsafe {
            core::arch::asm!("fence io, rw", options(nostack));
        }
        let notified = self.pending.get().is_some_and(|handler| {
            handler(crate::hart::current_hart())
                .expect("firmware IPI work could not complete")
        });
        if notified {
            // SAFETY: The fixed handler completed this hart's firmware work
            // under IRQ exclusion; Runtime owns supervisor IPI notification.
            unsafe { machine::InterruptPending::set_bit::<machine::SSIP>() };
        }
        notified
    }
}
