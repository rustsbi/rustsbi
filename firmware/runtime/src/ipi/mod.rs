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

enum Device {
    Software(Box<dyn IpiDevice<Software>>),
    External {
        device: Box<dyn IpiDevice<External>>,
        imsic: &'static Imsic,
    },
}

impl Ipi {
    /// Returns whether this service uses machine external interrupts.
    pub fn is_external(&self) -> bool {
        matches!(&self.device, Device::External { .. })
    }

    /// Constructs an IPI service using a machine software interrupt device.
    ///
    /// Returns [`Error::NotReady`] if the hart topology is not published.
    pub fn software(device: Box<dyn IpiDevice<Software>>) -> Result<Self> {
        Self::new(Device::Software(device))
    }

    /// Constructs an IPI service using a machine external interrupt device.
    ///
    /// Returns [`Error::NotReady`] if the hart topology or Runtime's IMSIC is
    /// not published.
    pub fn external(device: Box<dyn IpiDevice<External>>) -> Result<Self> {
        let imsic = imsic::get().ok_or(Error::NotReady)?;
        Self::new(Device::External { device, imsic })
    }
}

/// Configures supervisor state access for the selected IPI delivery mode.
///
/// Call after registering the platform IPI, if one exists,
/// so the selected mode is visible.
/// AIA state access is enabled only when `smaia` is exposed
/// and the registered IPI uses machine external interrupts.
pub fn configure_state_access(smaia: bool) {
    let enable_aia = smaia
        && matches!(
            IPI.get().map(|ipi| &ipi.device),
            Some(Device::External { .. })
        );
    crate::csr::stateen::configure_supervisor(enable_aia);
}

/// Registers the platform's single firmware IPI object.
///
/// A second registration returns [`Error::AlreadyInitialized`].
pub fn init(ipi: Ipi) -> Result<()> {
    let mut initialized = false;
    IPI.call_once(|| {
        initialized = true;
        ipi
    });
    if initialized {
        Ok(())
    } else {
        Err(Error::AlreadyInitialized)
    }
}

/// Returns the registered firmware IPI object.
///
/// Returns `None` before registration.
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
