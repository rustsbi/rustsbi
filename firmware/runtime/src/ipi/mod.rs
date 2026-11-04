//! Firmware IPI device ownership and interrupt delivery.

mod delivery;
mod event;
pub mod imsic;
mod mode;
mod request;

use crate::{
    Error, Result,
    hart::{HartId, PerHart},
};
use alloc::boxed::Box;
use core::sync::atomic::AtomicUsize;
pub use csr::aia::imsic::Iid;
pub use event::IpiEvent;
use imsic::Imsic;
pub use imsic::validate_iid_capacity;
pub(crate) use mode::InterruptMode;
use spin::Once;

static IPI: Once<Ipi> = Once::new();

/// Creates and publishes the firmware IPI service.
///
/// Returns [`Error::AlreadyInitialized`] if a service has already been published.
pub fn init(device: Option<Box<dyn IpiDevice>>) -> Result<&'static Ipi> {
    let mut initialized = false;
    let ipi = IPI.try_call_once(|| {
        let ipi = Ipi::new(device)?;
        initialized = true;
        Ok(ipi)
    })?;
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
    pending: PerHart<AtomicUsize>,
    handlers: [Once<fn(HartId)>; event::EVENT_COUNT],
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
        /// Number of interrupt identities implemented by the interrupt file.
        num_iids: u16,
    },
}

/// A validated, ordinary hart-mask window for one device send operation.
/// SBI broadcast requests must first be expanded into ordinary windows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IpiRequest {
    hart_mask: usize,
    hart_mask_base: usize,
}

/// Firmware IPIs delivered as machine software interrupts.
pub enum Software {}

/// Firmware IPIs delivered as machine external interrupts through an IMSIC.
pub enum External {}

/// Acknowledgement of the current hart's firmware IPI source.
///
/// [`Software`] devices use an empty `()` context.
/// [`External`] acknowledgement requires Runtime's privately held machine IMSIC.
pub trait Acknowledge<M: InterruptMode> {
    /// Acknowledges this hart's source before pending events are observed.
    ///
    /// Software devices return `true` after clearing their dedicated source.
    /// External devices return whether the claimed identity is the firmware IPI.
    /// Runtime invokes this in M-mode after initializing this hart's IPI state,
    /// with local machine interrupts disabled.
    fn acknowledge(&self, context: &M::Context) -> Result<bool>;
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
    /// Returns whether this device delivers through machine external interrupts.
    pub fn is_external(&self) -> bool {
        matches!(self.delivery(), Some(IpiDelivery::Imsic { .. }))
    }

    /// Returns the selected delivery path, or `None` without a device.
    pub fn delivery(&self) -> Option<IpiDelivery> {
        self.device.as_ref().map(|device| device.delivery())
    }
}
