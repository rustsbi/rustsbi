//! Firmware IPI interrupt protocol.
//!
//! Sending publishes an event before signalling its target.
//! Receiving acknowledges the hardware source before processing pending events.
//! Hart wakeup can also signal the hardware without an IPI event.
//!
//! A parked hart clears its source before reading its event,
//! because clearing after observing a stopped hart could erase a concurrent
//! start's only wakeup.
//!
//! The memory-ordering instructions are defined by the RISC-V ISA:
//! <https://docs.riscv.org/reference/isa/unpriv/rv32.html>.

use super::{Device, External, InterruptMode, Ipi, IpiEvent, Software};
use crate::{Error, Result, hart::HartId, irq::LocalIrqDisabled};
use csr::values::trap::Interrupt;

impl Ipi {
    /// Sends a firmware event to the target harts.
    ///
    /// Returns [`Error::NotReady`] if the event has no registered handler.
    /// Each target's event is published before the hardware is signalled.
    /// If a signal fails, later targets are not processed and the failed target's
    /// event may remain pending.
    /// A later IPI may deliver that event.
    pub fn send(&self, event: IpiEvent, targets: impl IntoIterator<Item = HartId>) -> Result<()> {
        if self.handler(event).is_none() {
            return Err(Error::NotReady);
        }
        for hart in targets {
            self.post(hart, event);
            // Signal even when an event was already pending: an earlier
            // device write may have failed.
            self.send_hardware_ipi(hart)?;
        }
        Ok(())
    }

    pub(crate) fn send_hardware_ipi(&self, hart: HartId) -> Result<()> {
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            // SAFETY: The fence only orders memory writes before device output.
            unsafe {
                core::arch::asm!("fence w, o", options(nostack));
            }
            match &self.device {
                Device::Software(device) => device.send_ipi(hart),
                Device::External { device, .. } => device.send_ipi(hart),
            }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = hart;
            unimplemented!("IPI delivery requires RISC-V")
        }
    }

    pub(crate) fn acknowledge(&self, _irq: &LocalIrqDisabled) -> Result<bool> {
        match &self.device {
            Device::Software(device) => device.acknowledge(&()),
            Device::External { device, imsic } => device.acknowledge(imsic),
        }
    }

    /// Acknowledges `source` only when it belongs to the selected device.
    ///
    /// A source for the other delivery mode is left pending and reported as
    /// not acknowledged.
    pub(crate) fn acknowledge_source(
        &self,
        source: Interrupt,
        irq: &LocalIrqDisabled,
    ) -> Result<bool> {
        match (&self.device, source) {
            (Device::Software(_), Interrupt::MachineSoftware)
            | (Device::External { .. }, Interrupt::MachineExternal) => self.acknowledge(irq),
            _ => Ok(false),
        }
    }

    /// Initializes this hart's IPI state after Runtime installs its trap entry.
    pub(crate) fn init_hart(&self, irq: &LocalIrqDisabled) -> Result<()> {
        if let Device::External { imsic, .. } = &self.device {
            imsic.init_hart(irq)?;
        }
        self.acknowledge(irq).map(|_| ())
    }

    pub(crate) fn enable_interrupt(&self, irq: &LocalIrqDisabled) {
        match &self.device {
            Device::Software(_) => Software::enable_interrupt(irq),
            Device::External { .. } => External::enable_interrupt(irq),
        }
    }
}
