//! Existing architectural-time emulation and direct platform counter reads.

use crate::csr::csrr;
use csr::user;

#[derive(Clone, Copy)]
pub(super) enum CounterSource {
    Device,
    DeviceWords,
    Architectural,
}

impl super::Timer {
    pub(super) fn counter_source(&self) -> Option<CounterSource> {
        [
            CounterSource::Device,
            CounterSource::DeviceWords,
            CounterSource::Architectural,
        ]
        .into_iter()
        .find(|source| self.read_counter(*source).is_some())
    }

    pub(super) fn read_counter(&self, source: CounterSource) -> Option<u64> {
        let machine = self.machine()?;
        match source {
            CounterSource::Device => return machine.read_time(),
            CounterSource::DeviceWords => {
                #[cfg(target_pointer_width = "64")]
                return machine.read_time_low().map(|time| time as u64);
                #[cfg(target_pointer_width = "32")]
                loop {
                    let high = machine.read_time_high()?;
                    let low = machine.read_time_low()?;
                    if high == machine.read_time_high()? {
                        return Some(((high as u64) << 32) | low as u64);
                    }
                }
            }
            CounterSource::Architectural => {}
        }
        #[cfg(target_pointer_width = "64")]
        {
            csrr!(user::Time).ok().map(|time| time as u64)
        }
        #[cfg(target_pointer_width = "32")]
        {
            loop {
                let high = csrr!(user::TimeHigh).ok()?;
                let low = csrr!(user::Time).ok()?;
                if high == csrr!(user::TimeHigh).ok()? {
                    return Some(((high as u64) << 32) | low as u64);
                }
            }
        }
    }

    /// Reads the direct platform counter without probing architectural time.
    /// Returns the full counter on RV64, or its lower word on RV32.
    pub fn read_time_low(&self) -> Option<usize> {
        self.machine()?.read_time_low()
    }

    /// Reads `time` for trap emulation after checking lower-mode permissions.
    pub(crate) fn read_time(&self) -> Option<usize> {
        if let Some(time) = self.read_time_low() {
            return Some(time);
        }
        // Trap dispatch installed recovery; unavailable CSR access can fall back.
        if let Ok(time) = csrr!(user::Time) {
            return Some(time);
        }
        self.machine()?.read_time().map(|time| time as usize)
    }

    /// Reads RV32 `timeh` through the same counter-source policy as `time`.
    #[cfg(target_pointer_width = "32")]
    pub(crate) fn read_time_high(&self) -> Option<usize> {
        if let Some(time) = self.machine().and_then(|machine| machine.read_time_high()) {
            return Some(time);
        }
        if let Ok(time) = csrr!(user::TimeHigh) {
            return Some(time);
        }
        self.machine()?
            .read_time()
            .map(|time| (time >> 32) as usize)
    }
}
