//! Firmware IPI device ownership and interrupt delivery.

use crate::{Error, Result, hart::HartId, irq::LocalIrqDisabled};
use alloc::boxed::Box;
pub use csr::aia::imsic::Iid;
use csr::{aia::machine::Mtopei, machine};

/// Firmware IPI send capability, issued during platform initialization.
pub struct Ipi {
    device: Option<Box<dyn IpiDevice>>,
    deliver: fn() -> bool,
}

/// Hardware delivery path for the platform's firmware IPIs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpiDelivery {
    /// Machine software interrupts, acknowledged through the device's MMIO.
    Software,
    /// Machine external interrupts, claimed through the current hart's IMSIC.
    Imsic {
        /// The fixed firmware IPI interrupt identity.
        iid: Iid,
    },
}

/// A validated, ordinary hart-mask window for one device send operation.
/// SBI broadcast requests must first be expanded into ordinary windows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IpiRequest {
    hart_mask: usize,
    hart_mask_base: usize,
}

impl IpiRequest {
    /// Validates every target against Runtime's published hart topology.
    /// Returns an error for broadcast encoding, overflow, or an unknown hart.
    pub fn from_mask_base(hart_mask: usize, hart_mask_base: usize) -> Result<Self> {
        if hart_mask_base == usize::MAX {
            return Err(Error::InvalidArgs);
        }
        for bit in crate::rustsbi::HartMask::from_mask_base(hart_mask, 0) {
            let raw = hart_mask_base.checked_add(bit).ok_or(Error::InvalidArgs)?;
            HartId::from_raw(raw)?;
        }
        Ok(Self {
            hart_mask,
            hart_mask_base,
        })
    }

    /// Creates a window containing one validated hart.
    pub fn for_hart(hart: HartId) -> Self {
        let raw = hart.as_usize();
        let bit = raw % usize::BITS as usize;
        Self {
            hart_mask: 1 << bit,
            hart_mask_base: raw - bit,
        }
    }

    /// Iterates the validated hardware hart IDs.
    pub fn harts(self) -> impl Iterator<Item = usize> {
        crate::rustsbi::HartMask::from_mask_base(self.hart_mask, self.hart_mask_base).into_iter()
    }

    /// Merges another window with the same base, retaining validated targets.
    pub fn try_merge(&mut self, other: Self) -> bool {
        if self.hart_mask_base != other.hart_mask_base {
            return false;
        }
        self.hart_mask |= other.hart_mask;
        true
    }
}

/// Platform operations for firmware IPI delivery.
/// Calls may run concurrently on different harts.
pub trait IpiDevice: Send + Sync {
    /// Sends machine IPIs to one validated target window.
    /// On failure, some targets may already have been signaled.
    fn send(&self, request: IpiRequest) -> Result<()>;

    /// Acknowledges this hart's software-interrupt source through the device.
    /// Returns whether it was a firmware IPI. Runtime claims IMSIC through CSR.
    fn acknowledge(&self) -> Result<bool> {
        Err(Error::NotSupported)
    }

    /// Returns the hardware delivery path, fixed after publication.
    fn delivery(&self) -> IpiDelivery {
        IpiDelivery::Software
    }
}

impl Ipi {
    /// Creates an IPI service that owns the optional device.
    /// Runtime clears each hart's pending source after publication.
    /// The callback handles pending SBI work after acknowledgement, with MIE disabled.
    /// Return `true` to request a supervisor software interrupt.
    pub fn new(device: Option<Box<dyn IpiDevice>>, deliver: fn() -> bool) -> Self {
        Self { device, deliver }
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

    /// Returns the selected delivery path, or `None` without a device.
    pub fn delivery(&self) -> Option<IpiDelivery> {
        self.device.as_ref().map(|device| device.delivery())
    }

    pub(crate) fn init_current(&self, irq: &LocalIrqDisabled) -> Result<()> {
        if !crate::boot::traps_ready() {
            return Err(Error::NotReady);
        }
        self.clear_current(irq)
    }

    pub(crate) fn clear_current(&self, _irq: &LocalIrqDisabled) -> Result<()> {
        match self.device.as_deref() {
            Some(device) => match device.delivery() {
                IpiDelivery::Software => device.acknowledge().map(|_| ()),
                IpiDelivery::Imsic { iid } => {
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
        unsafe { Mtopei::claim() }.iid() == Some(iid)
    }

    pub(crate) fn claim_external(&self, _irq: &LocalIrqDisabled) -> bool {
        match self.delivery() {
            Some(IpiDelivery::Imsic { iid }) => self.claim_imsic(iid),
            _ => false,
        }
    }

    /// Enables hart wake sources without changing MIE or timer delivery.
    pub(crate) fn prepare_wakeup(&self, _irq: &LocalIrqDisabled) {
        // SAFETY: Boot installed Runtime entry before configuring wake sources.
        // The caller's IRQ context excludes interrupt reentry during this update.
        unsafe {
            machine::InterruptEnable::modify(|_, value| {
                value.set::<machine::MSIE>(machine::Interrupt::Enabled);
                if matches!(self.delivery(), Some(IpiDelivery::Imsic { .. })) {
                    value.set::<machine::MEIE>(machine::Interrupt::Enabled);
                }
            });
        }
    }

    pub(crate) fn deliver_current(&self, _irq: &LocalIrqDisabled) {
        if self.device.is_some() && (self.deliver)() {
            // SAFETY: The fixed callback completed this hart's firmware work
            // under IRQ exclusion; Runtime owns supervisor IPI notification.
            unsafe { machine::InterruptPending::set_bit::<machine::SSIP>() };
        }
    }
}
