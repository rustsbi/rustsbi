//! Firmware IPI device ownership and interrupt delivery.

mod delivery;
mod imsic;

pub use imsic::Iid;
mod request;

use crate::{Error, Result, hart::HartId};
use alloc::boxed::Box;
use spin::Once;

static IPI: Once<Ipi> = Once::new();

/// Creates and installs the firmware IPI device.
pub fn init(device: Option<Box<dyn IpiDevice>>) -> &'static Ipi {
    IPI.call_once(|| Ipi::from_device(device))
}

/// Returns the installed firmware IPI device.
pub fn get() -> Option<&'static Ipi> {
    IPI.get()
}

/// Firmware IPI send capability, issued during platform initialization.
pub struct Ipi {
    device: Option<Box<dyn IpiDevice>>,
    pending: Once<fn(HartId) -> Result<bool>>,
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
    fn from_device(device: Option<Box<dyn IpiDevice>>) -> Self {
        Self {
            device,
            pending: Once::new(),
        }
    }

    /// Registers the consumer that processes pending firmware IPI work.
    ///
    /// The handler runs after device acknowledgement with machine interrupts
    /// disabled. It returns whether a supervisor software interrupt is required.
    /// A handler error prevents resuming lower-mode execution after incomplete
    /// IPI work.
    pub fn register_pending(&self, handler: fn(HartId) -> Result<bool>) -> Result<()> {
        let mut installed = false;
        self.pending.call_once(|| {
            installed = true;
            handler
        });
        if installed {
            Ok(())
        } else {
            Err(Error::AlreadyInitialized)
        }
    }

    /// Returns the selected delivery path, or `None` without a device.
    pub fn delivery(&self) -> Option<IpiDelivery> {
        self.device.as_ref().map(|device| device.delivery())
    }
}
