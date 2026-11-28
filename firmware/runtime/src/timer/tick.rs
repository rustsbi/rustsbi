//! Selection of this hart's time CSR or the platform timer device.

use super::TimerDevice;
use crate::{Error, Result, irq};
use csr::{Read, standard::isa::unprivileged::Time};
use spin::Once;

pub(super) enum Tick {
    /// The architectural `time` CSR.
    Csr,
    /// A counter read through the registered machine timer device.
    Device,
}

impl Tick {
    pub(super) fn get(slot: &Once<Self>) -> Result<&Self> {
        if let Some(tick) = slot.get() {
            return Ok(tick);
        }
        irq::with_local_disabled(|_| {
            slot.try_call_once(|| {
                if !crate::recovery::is_ready() {
                    return Err(Error::NotReady);
                }
                // SAFETY: Recovery is ready and local interrupts cannot reenter this probe.
                let present = unsafe { Time::try_read() }.is_ok();
                Ok(if present { Self::Csr } else { Self::Device })
            })
        })
    }

    pub(super) fn read(&self, device: Option<&dyn TimerDevice>) -> Result<Option<u64>> {
        match self {
            Self::Csr => Ok(Some(Time::read())),
            Self::Device => match device {
                Some(device) => device.read_time(),
                None => Ok(None),
            },
        }
    }
}
