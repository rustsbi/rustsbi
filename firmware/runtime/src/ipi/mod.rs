//! Firmware IPI ownership and interrupt delivery.

mod delivery;
mod event;
pub mod imsic;
mod mode;

use crate::{
    Error, Result,
    hart::{HartId, PerHart},
    irq,
};
use alloc::boxed::Box;
use core::sync::atomic::AtomicUsize;
pub use csr::aia::imsic::Iid;
pub use event::IpiEvent;
use imsic::Imsic;
pub(crate) use mode::InterruptMode;
use spin::Once;

static IPI: Once<Ipi> = Once::new();

/// Firmware IPI delivery through the platform's selected interrupt mode.
///
/// The device and local interrupt operations stay in Runtime.
pub struct Ipi {
    device: Device,
    pending: PerHart<AtomicUsize>,
    handlers: [Once<fn(HartId)>; event::EVENT_COUNT],
}

/// A platform's firmware IPI sender.
pub enum Device {
    /// A machine software interrupt device.
    Software(Box<dyn IpiDevice<Software>>),
    /// A machine external interrupt device backed by an IMSIC.
    External(Box<dyn IpiDevice<External>>),
}

impl Ipi {
    /// Returns whether this device delivers through machine external interrupts.
    pub fn is_external(&self) -> bool {
        matches!(&self.device, Device::External(_))
    }
}

/// Registers the platform's single firmware IPI object.
///
/// The hart topology must already be published.
/// External delivery requires [`imsic::init`] first.
/// The first registration returns [`Error::NotReady`] if either requirement is unmet.
/// A second registration returns [`Error::AlreadyInitialized`].
pub fn init(device: Device) -> Result<()> {
    let mut initialized = false;
    IPI.try_call_once(|| {
        if matches!(&device, Device::External(_)) && imsic::get().is_none() {
            return Err(Error::NotReady);
        }
        let ipi = Ipi::new(device)?;
        initialized = true;
        Ok(ipi)
    })?;
    if initialized {
        Ok(())
    } else {
        Err(Error::AlreadyInitialized)
    }
}

/// Returns the registered firmware IPI object.
///
/// Returns `None` before registration or when there is no IPI device.
pub fn get() -> Option<&'static Ipi> {
    IPI.get()
}

/// Firmware IPIs delivered as machine software interrupts.
pub enum Software {}

/// Firmware IPIs delivered as machine external interrupts through an IMSIC.
pub enum External {}

/// Acknowledgement of the current hart's firmware IPI source.
///
/// [`Software`] devices use an empty `()` context.
/// [`External`] devices use Runtime's fixed implementation, which requires
/// its privately held machine IMSIC.
pub trait Acknowledge<M: InterruptMode> {
    /// Acknowledges this hart's source before pending events are observed.
    ///
    /// Software devices return `true` after clearing their dedicated source.
    /// External devices return whether the claimed identity is the firmware IPI.
    /// Runtime invokes this in M-mode after initializing this hart's IPI state,
    /// with local machine interrupts disabled.
    fn acknowledge(&self, context: &M::Context) -> Result<bool>;
}

/// A platform controller that sends firmware IPIs through interrupt mode `M`.
///
/// Calls may run concurrently on different harts.
/// A failed signal does not guarantee delivery; callers decide whether to
/// report or retry.
/// Software devices must also implement [`Acknowledge<Software>`].
pub trait IpiDevice<M: InterruptMode>: Acknowledge<M> + Send + Sync {
    /// Sends a hardware IPI to `hart`.
    fn send_ipi(&self, hart: HartId) -> Result<()>;
}

impl<T: IpiDevice<External> + ?Sized> Acknowledge<External> for T {
    fn acknowledge(&self, imsic: &Imsic) -> Result<bool> {
        irq::with_local_disabled(|irq| Ok(imsic.claim(irq)))
    }
}
