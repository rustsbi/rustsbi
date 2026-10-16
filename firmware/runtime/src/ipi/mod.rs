//! Firmware IPI device ownership and interrupt delivery.

mod delivery;
mod imsic;
mod request;

use crate::{Error, Result};
use alloc::boxed::Box;
pub use csr::aia::imsic::Iid;
use spin::Once;

static IPI: Once<Ipi> = Once::new();

/// Creates and publishes the firmware IPI service.
///
/// The callback runs after a device interrupt is acknowledged. Callers must
/// publish the callback's state before sending IPIs or enabling delivery.
/// Returns [`Error::AlreadyInitialized`] if a service has already been published.
pub fn init(device: Option<Box<dyn IpiDevice>>, deliver: fn() -> bool) -> Result<&'static Ipi> {
    let mut initialized = false;
    let ipi = IPI.call_once(|| {
        initialized = true;
        Ipi::new(device, deliver)
    });
    if initialized {
        Ok(ipi)
    } else {
        Err(Error::AlreadyInitialized)
    }
}

/// Returns the published firmware IPI service.
pub fn get() -> Option<&'static Ipi> {
    IPI.get()
}

/// Firmware IPI send capability, issued during platform initialization.
pub struct Ipi {
    device: Option<Box<dyn IpiDevice>>,
    deliver_fn: fn() -> bool,
}

/// Hardware delivery path for the platform's firmware IPIs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpiDelivery {
    /// Machine software interrupts, acknowledged through the device's MMIO.
    Software,
    /// Machine external interrupts, claimed through the current hart's IMSIC.
    Imsic {
        /// The fixed firmware IPI interrupt identity.
        iid: Iid,
        /// Number of interrupt identities described by the interrupt file.
        num_ids: u16,
    },
}

/// A validated, ordinary hart-mask window for one device send operation.
/// SBI broadcast requests must first be expanded into ordinary windows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IpiRequest {
    hart_mask: usize,
    hart_mask_base: usize,
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
    pub fn new(device: Option<Box<dyn IpiDevice>>, deliver_fn: fn() -> bool) -> Self {
        Self { device, deliver_fn }
    }

    /// Returns the selected delivery path, or `None` without a device.
    pub fn delivery(&self) -> Option<IpiDelivery> {
        self.device.as_ref().map(|device| device.delivery())
    }
}
