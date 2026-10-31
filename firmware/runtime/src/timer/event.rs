//! One-shot firmware timer events on the current hart.

use super::{Timer, schedule::TimerTarget};
use crate::{
    Error, Result,
    hart::HartId,
    irq::{self, LocalIrqDisabled},
};
use alloc::boxed::Box;

/// A firmware timer event bound to the hart that created it.
///
/// An event shares the native machine comparator.
/// The callback runs on the bound hart with machine interrupts disabled.
/// It must not wait for another local interrupt.
/// It may set another deadline through the event passed to it.
///
/// The event and its callback remain registered for the firmware lifetime.
/// Cancellation stops the event but does not release its registration.
/// Stopping the hart cancels its pending event;
/// starting it again retains the event registration.
pub struct TimerEvent {
    timer: &'static Timer,
    hart: HartId,
    callback: Box<dyn Fn(&TimerEvent) + Send + Sync>,
}

impl Timer {
    /// Creates a one-shot firmware event on the current hart.
    ///
    /// The supervisor timer must already be initialized on this hart.
    /// Returns [`Error::NotReady`] if the native comparator exists
    /// but the supervisor timer is not initialized.
    /// Returns [`Error::Busy`] if an event is already registered.
    /// Returns [`Error::NotSupported`] if no native comparator
    /// or readable counter is available.
    /// The selected counter must remain readable and use the comparator's time base.
    /// Runtime panics during interrupt handling if that counter becomes unreadable.
    pub fn create_event<F>(&'static self, callback: F) -> Result<&'static TimerEvent>
    where
        F: Fn(&TimerEvent) + Send + Sync + 'static,
    {
        irq::with_local_disabled(|_| {
            let hart = HartId::current()?;
            self.machine().ok_or(Error::NotSupported)?;
            let local = self.local.get(hart);
            local.supervisor.get().ok_or(Error::NotReady)?;
            if local.event.get().is_some() {
                return Err(Error::Busy);
            }
            let selected_clock = local.state.lock().clock;
            let clock = selected_clock
                .filter(|clock| self.read_counter(*clock).is_some())
                .or_else(|| self.counter_source())
                .ok_or(Error::NotSupported)?;
            local.state.lock().clock = Some(clock);
            let event = local.event.call_once(|| TimerEvent {
                timer: self,
                hart,
                callback: Box::new(callback),
            });
            Ok(event)
        })
    }
}

impl TimerEvent {
    /// Sets an absolute deadline in the machine timer's counter units.
    ///
    /// This replaces any previous deadline.
    /// `u64::MAX` cancels the event.
    /// Returns [`Error::InvalidState`] when called from another hart.
    pub fn set_deadline(&self, deadline: u64) -> Result<()> {
        if deadline == u64::MAX {
            return self.cancel();
        }
        irq::with_local_disabled(|irq| {
            self.check_hart()?;
            self.timer
                .update_deadline(self.hart, TimerTarget::Firmware, Some(deadline), irq);
            Ok(())
        })
    }

    /// Cancels the pending deadline without releasing the event.
    ///
    /// Returns [`Error::InvalidState`] when called from another hart.
    pub fn cancel(&self) -> Result<()> {
        irq::with_local_disabled(|irq| {
            self.check_hart()?;
            self.timer
                .update_deadline(self.hart, TimerTarget::Firmware, None, irq);
            Ok(())
        })
    }

    pub(super) fn deliver(&self, _irq: &LocalIrqDisabled) {
        (self.callback)(self);
    }

    fn check_hart(&self) -> Result<()> {
        if HartId::current()? == self.hart {
            Ok(())
        } else {
            Err(Error::InvalidState)
        }
    }
}
