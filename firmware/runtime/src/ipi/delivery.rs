//! IPI publication, acknowledgement, and local interrupt delivery.

use super::{
    External, Iid, InterruptMode, Ipi, IpiDelivery, IpiEvent, IpiRequest, Software, imsic,
};
use crate::{Error, Result, hart::HartId, irq::LocalIrqDisabled};
use csr::aia::machine::Mtopei;

impl Ipi {
    /// Sends a firmware event to the target harts.
    pub fn send_event(
        &self,
        event: IpiEvent,
        targets: impl IntoIterator<Item = HartId>,
    ) -> Result<()> {
        if self.handler(event).is_none() {
            return Err(Error::NotReady);
        }
        for hart in targets {
            self.post(hart, event);
            self.send_hardware_ipi(hart)?;
        }
        Ok(())
    }

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

    pub(crate) fn send_hardware_ipi(&self, hart: HartId) -> Result<()> {
        self.send(IpiRequest::for_hart(hart))
    }

    pub(crate) fn init_hart(&self, irq: &LocalIrqDisabled) -> Result<()> {
        if !crate::boot::traps_ready() {
            return Err(Error::NotReady);
        }
        if let Some(IpiDelivery::Imsic { iid, num_iids }) = self.delivery() {
            imsic::initialize(num_iids, iid, irq)?;
        }
        self.acknowledge(irq).map(|_| ())
    }

    pub(crate) fn acknowledge(&self, _irq: &LocalIrqDisabled) -> Result<bool> {
        match self.device.as_deref() {
            Some(device) => match device.delivery() {
                IpiDelivery::Software => device.acknowledge(),
                IpiDelivery::Imsic { iid, .. } => Ok(self.claim_imsic(iid)),
            },
            None => Ok(false),
        }
    }

    #[inline]
    fn claim_imsic(&self, iid: Iid) -> bool {
        // SAFETY: The selected IMSIC device establishes Mtopei support on this
        // hart. The instruction atomically claims the reported interrupt;
        // callers serialize the surrounding firmware IPI delivery sequence.
        unsafe { Mtopei::claim() }.iid() == Some(iid)
    }

    pub(crate) fn enable_interrupt(&self, irq: &LocalIrqDisabled) {
        match self.delivery() {
            Some(IpiDelivery::Software) => Software::enable_interrupt(irq),
            Some(IpiDelivery::Imsic { .. }) => External::enable_interrupt(irq),
            None => {}
        }
    }
}
