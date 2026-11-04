//! Registration and dispatch of firmware IPI events.

use super::{Ipi, IpiDevice};
use crate::{
    Error, Result, events,
    hart::{self, HartId, PerHart},
    instruction,
    irq::LocalIrqDisabled,
};
use alloc::boxed::Box;
use core::sync::atomic::{
    AtomicUsize,
    Ordering::{Acquire, Release},
};
use csr::machine;
use spin::Once;

/// An event that can be sent through the firmware IPI service.
///
/// Pending requests for the same event may be coalesced.
/// A handler that owns a work queue must drain it before returning.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub enum IpiEvent {
    /// A request for a supervisor software interrupt.
    Supervisor,
    /// A request to process the firmware's remote-fence queue.
    Rfence,
}

pub(super) const EVENT_COUNT: usize = 2;
// Process the snapshot's firmware work before requesting a supervisor interrupt.
const PROCESS_ORDER: [IpiEvent; EVENT_COUNT] = [IpiEvent::Rfence, IpiEvent::Supervisor];
const _: () = assert!(EVENT_COUNT <= usize::BITS as usize);

impl IpiEvent {
    fn mask(self) -> usize {
        1usize << self as usize
    }
}

fn handle_supervisor(_hart: HartId) {
    // SAFETY:
    // 1. Runtime invokes the handler on the target hart in M-mode.
    // 2. Runtime owns this hart's SSIP and excludes local interrupt reentry.
    unsafe { machine::InterruptPending::set_bit::<machine::SSIP>() };
}

impl Ipi {
    pub(super) fn new(device: Option<Box<dyn IpiDevice>>) -> Result<Self> {
        let ipi = Self {
            device,
            pending: PerHart::new(|_| AtomicUsize::new(0))?,
            handlers: [const { Once::new() }; EVENT_COUNT],
        };
        ipi.handlers[IpiEvent::Supervisor as usize].call_once(|| handle_supervisor);
        Ok(ipi)
    }

    /// Registers a permanent handler for a firmware IPI event.
    ///
    /// Runtime invokes the handler on the target hart in M-mode with local
    /// machine interrupts disabled.
    /// The supervisor event is registered by Runtime.
    /// Returns [`Error::AlreadyInitialized`] if the event already has a handler.
    pub fn register(&self, event: IpiEvent, handler: fn(HartId)) -> Result<()> {
        let mut registered = false;
        self.handlers[event as usize].call_once(|| {
            registered = true;
            handler
        });
        if registered {
            Ok(())
        } else {
            Err(Error::AlreadyInitialized)
        }
    }

    pub(super) fn handler(&self, event: IpiEvent) -> Option<fn(HartId)> {
        self.handlers[event as usize].get().copied()
    }

    pub(super) fn post(&self, hart: HartId, event: IpiEvent) {
        self.pending.get(hart).fetch_or(event.mask(), Release);
    }

    pub(crate) fn process_pending(&self, _irq: &LocalIrqDisabled) {
        instruction::fence();
        let hart = hart::current_hart();
        let pending = self.pending.get(hart).swap(0, Acquire);
        if pending & IpiEvent::Supervisor.mask() != 0
            && let Some(counters) = events::get()
        {
            counters.record_supervisor_ipi();
        }
        for event in PROCESS_ORDER {
            if pending & event.mask() != 0 {
                let handler = self
                    .handler(event)
                    .expect("BUG: pending IPI event has no registered handler");
                handler(hart);
            }
        }
    }
}
