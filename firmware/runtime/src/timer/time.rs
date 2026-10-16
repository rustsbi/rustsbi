//! Existing architectural-time emulation and direct platform counter reads.

use crate::csr::csrr;
use csr::user;

impl super::Timer {
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
