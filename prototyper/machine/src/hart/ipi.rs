//! Machine-interrupt capability and its private physical-device role.

use alloc::sync::Arc;

use super::instructions::current_hart_id;
use super::protocol::{HartAdmission, HartNotifications, map_ipi_error};
use crate::hart::HartTargets;

/// Physical mechanism used to wake a hart for machine-owned work.
///
/// Implementations bind one validated interrupt source and translate physical
/// hart IDs to device-specific targets. The protocol state remains owned by
/// `HartAdmission`; a device only rings or acknowledges the selected source.
pub(crate) trait IpiDevice: Send + Sync {
    /// Initializes the calling hart's notification endpoint before use.
    fn prepare_current_hart(&self) -> Result<(), IpiError> {
        Ok(())
    }

    /// Rings the endpoint associated with `hart_id`.
    ///
    /// Invalid IDs have already been rejected by admission. Implementations
    /// must not retain a reference to protocol state or wait for completion.
    fn notify(&self, hart_id: usize);

    /// Acknowledges the device notification delivered to `hart_id`.
    fn claim(&self, hart_id: usize);

    /// Identifies the machine interrupt cause driven by this device.
    fn notification(&self) -> Notification {
        Notification::Software
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Notification {
    Software,
    External,
}

impl Notification {
    pub(crate) const fn machine_interrupt_bit(self) -> usize {
        match self {
            Self::Software => 1 << 3,
            Self::External => 1 << 11,
        }
    }
}

/// Failure from an IPI request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IpiError {
    /// A selected architectural hart does not exist or is unavailable.
    InvalidHart,
    /// The machine notification mechanism could not complete the request.
    Failed,
}

/// Authority to request machine-mediated work on admitted harts.
///
/// Raw software-interrupt registers and device indices are not exposed.
pub struct Ipi {
    admission: Arc<HartAdmission>,
}

impl Ipi {
    pub(crate) fn new(admission: Arc<HartAdmission>) -> Self {
        Self { admission }
    }

    /// Sends one coalescible supervisor IPI to every validated target.
    pub fn send(&self, targets: HartTargets) -> Result<(), IpiError> {
        self.admission.send(targets)
    }
}

impl HartAdmission {
    /// Commits a coalescible supervisor IPI before ringing physical targets.
    pub(crate) fn send(&self, targets: HartTargets) -> Result<(), IpiError> {
        let current_hart = current_hart_id();
        let (current, resolved, notifications) = {
            let mut state = self.state.lock();
            let current = state
                .resolve_physical(current_hart)
                .map_err(map_ipi_error)?;
            let resolved = state.resolve_targets(targets).map_err(map_ipi_error)?;
            state.commit_ipi(resolved).map_err(map_ipi_error)?;
            let notifications = HartNotifications::from_state(&state, current, resolved);
            (current, resolved, notifications)
        };
        self.notify(notifications);
        if resolved.contains(current) {
            self.drain(current_hart, false)?;
        }
        Ok(())
    }

    /// Initializes the calling hart's physical notification endpoint.
    pub(crate) fn prepare_current_hart(&self) -> Result<(), IpiError> {
        self.device.prepare_current_hart()
    }

    /// Returns the machine interrupt cause used for work notification.
    pub(crate) fn notification(&self) -> Notification {
        self.device.notification()
    }

    /// Acknowledges one device notification and drains its committed work.
    pub(crate) fn handle_notification(&self) -> Result<(), IpiError> {
        self.drain(current_hart_id(), true).map(|_| ())
    }

    /// Checks whether a trap cause belongs to the installed notification device.
    pub(crate) fn handles_notification(&self, notification: Notification) -> bool {
        self.device.notification() == notification
    }
}
