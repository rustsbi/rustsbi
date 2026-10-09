//! Hart-local firmware IPI transport and remote notification capabilities.
//!
//! Platform policy supplies one shared device. Runtime owns initialization,
//! acknowledgement, ordering, and delivery; firmware queues receive only a
//! send capability.

#![deny(unsafe_code)]

use crate::csr::{Mie, Mip};
use core::{fmt, marker::PhantomData};
use spin::Once;

use crate::hart::{self, HartEvent, HartId};
mod imsic;
pub use imsic::{ImsicError, ImsicInterruptFile};

static SERVICE: Once<Service> = Once::new();

/// A platform IPI operation failed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IpiError;

/// Platform hardware used to notify harts and describe its interrupt source.
pub trait IpiDevice: Send + Sync {
    /// Sends a machine IPI to `hart`.
    fn send(&self, hart: HartId) -> Result<(), IpiError>;

    /// Describes how Runtime receives interrupts from this device.
    ///
    /// Runtime snapshots this description when publishing the device. Describing
    /// an IMSIC file does not access its hart-local CSRs; Runtime owns those
    /// operations.
    fn interrupt_source(&self) -> InterruptSource<'_>;
}

/// A machine software-interrupt device with a hart-local MMIO source.
pub trait SoftwareInterruptDevice: Send + Sync {
    /// Clears `hart`'s pending machine software interrupt.
    fn clear(&self, hart: HartId) -> Result<(), IpiError>;
}

/// The architectural interrupt source of a platform IPI device.
#[derive(Clone, Copy)]
pub enum InterruptSource<'a> {
    /// CLINT or another machine software-interrupt device.
    Software(&'a dyn SoftwareInterruptDevice),
    /// The validated machine interrupt file of an IMSIC device.
    Imsic(ImsicInterruptFile),
}

/// Remote notification access to a shared platform IPI device.
pub struct IpiSender {
    device: &'static dyn IpiDevice,
}

impl IpiSender {
    /// Grants remote notification access without local acknowledgement rights.
    pub fn new(device: &'static dyn IpiDevice) -> Self {
        Self { device }
    }

    /// Publishes pending firmware work before notifying `hart`.
    pub fn send(&self, hart: HartId) -> Result<(), IpiError> {
        crate::instructions::memory_to_io();
        self.device.send(hart)
    }
}

/// Work completed after Runtime acknowledges and orders the machine source.
pub trait IpiHandler: Sync {
    /// Delivers queued work and reports whether supervisor SSIP must be set.
    fn deliver_current(&self) -> bool;
}

struct Service {
    device: &'static dyn IpiDevice,
    source: InterruptSource<'static>,
    handler: &'static dyn IpiHandler,
}

/// Failure to acquire or operate the current hart's IPI capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// No firmware IPI source has been published.
    Unavailable,
    /// The capability is used outside its configured hart.
    InvalidHartId,
    /// The MMIO software-interrupt source could not be acknowledged.
    Device(IpiError),
    /// An architectural IMSIC operation failed.
    Imsic(ImsicError),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable => formatter.write_str("firmware IPI source unavailable"),
            Self::InvalidHartId => formatter.write_str("invalid current IPI hart"),
            Self::Device(error) => {
                write!(formatter, "IPI device acknowledgement failed: {error:?}")
            }
            Self::Imsic(error) => write!(formatter, "IMSIC operation failed: {error:?}"),
        }
    }
}

/// Access to the configured hart's machine IPI receive path.
///
/// Runtime grants this capability after source publication. It cannot move
/// between harts or be shared; remote senders have no receive operations.
pub struct Ipi {
    hart: HartId,
    service: &'static Service,
    _local: PhantomData<*mut ()>,
}

impl Ipi {
    /// Publishes the device, its fixed receive source, and work handler together.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error::AlreadyInitialized`] after the first installation.
    pub fn install(
        device: &'static dyn IpiDevice,
        handler: &'static dyn IpiHandler,
    ) -> crate::Result<()> {
        let mut installed = false;
        SERVICE.call_once(|| {
            installed = true;
            Service {
                device,
                source: device.interrupt_source(),
                handler,
            }
        });
        if installed {
            Ok(())
        } else {
            Err(crate::Error::AlreadyInitialized)
        }
    }

    /// Acquires local receive access for a hart in the published topology.
    pub fn current() -> Result<Self, Error> {
        let hart = HartId::current().map_err(|_| Error::InvalidHartId)?;
        let service = SERVICE.get().ok_or(Error::Unavailable)?;
        Ok(Self {
            hart,
            service,
            _local: PhantomData,
        })
    }

    /// Publishes pending firmware work before notifying `hart`.
    pub(crate) fn send(&self, hart: HartId) -> Result<(), IpiError> {
        IpiSender::new(self.service.device).send(hart)
    }

    pub(crate) fn initialize(&self) -> Result<(), Error> {
        self.check_current()?;
        match self.service.source {
            InterruptSource::Software(device) => device.clear(self.hart).map_err(Error::Device)?,
            InterruptSource::Imsic(file) => file.initialize_current_hart().map_err(Error::Imsic)?,
        }
        crate::instructions::io_to_memory();
        self.prepare_wait()
    }

    fn check_current(&self) -> Result<(), Error> {
        if HartId::current().map_err(|_| Error::InvalidHartId)? != self.hart {
            return Err(Error::InvalidHartId);
        }
        Ok(())
    }

    fn acknowledge(&self) -> Result<bool, Error> {
        self.check_current()?;
        let claimed = match self.service.source {
            InterruptSource::Software(device) => {
                device.clear(self.hart).map_err(Error::Device)?;
                true
            }
            InterruptSource::Imsic(file) => file.acknowledge_current().map_err(Error::Imsic)?,
        };
        crate::instructions::io_to_memory();
        Ok(claimed)
    }

    fn deliver(&self) {
        if self.service.handler.deliver_current() {
            Mip::set_bits(Mip::SUPERVISOR_SOFTWARE);
        }
    }

    /// Reconciles pending work before the local hart sleeps or changes state.
    pub(crate) fn drain(&self) -> Result<(), Error> {
        self.acknowledge()?;
        self.deliver();
        Ok(())
    }

    /// Acknowledges the source before consuming lifecycle state and pending work.
    pub(crate) fn poll(&self) -> Result<HartEvent, Error> {
        self.acknowledge()?;
        let event = hart::take_local_event();
        self.deliver();
        Ok(event)
    }

    pub(crate) fn receive_software(&self) -> Result<Option<HartEvent>, Error> {
        if matches!(self.service.source, InterruptSource::Software(_)) {
            self.poll().map(Some)
        } else {
            Ok(None)
        }
    }

    pub(crate) fn receive_external(&self) -> Result<Option<HartEvent>, Error> {
        if matches!(self.service.source, InterruptSource::Imsic(_)) && self.acknowledge()? {
            let event = hart::take_local_event();
            self.deliver();
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn prepare_wait(&self) -> Result<(), Error> {
        self.check_current()?;
        match self.service.source {
            InterruptSource::Software(_) => {
                Mie::clear_bits(Mie::MACHINE_EXTERNAL);
                Mie::set_bits(Mie::MACHINE_SOFTWARE);
            }
            InterruptSource::Imsic(_) => {
                Mie::clear_bits(Mie::MACHINE_SOFTWARE);
                Mie::set_bits(Mie::MACHINE_EXTERNAL);
            }
        }
        Ok(())
    }
}
