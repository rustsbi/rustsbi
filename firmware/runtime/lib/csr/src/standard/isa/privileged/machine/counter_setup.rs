//! Machine counter setup.
use crate::{
    macros::csr,
    value::Value,
    values::counter::{HardwareCounter, InhibitedCounters},
};

csr! {
    /// Machine counter-inhibit register (`mcountinhibit`).
    ///
    /// See
    /// <https://docs.riscv.org/reference/isa/priv/machine.html#mcountinhibit>.
    CounterInhibit { 0x320, r: InhibitedCounters }
}

impl CounterInhibit {
    /// Stops programmable counters during initial hart setup or PMU reset.
    /// Clears CY and IR so cycle and instret continue running.
    ///
    /// # Safety
    /// Own counter initialization and coordinate any shared cycle counter.
    pub unsafe fn initialize_programmable() {
        // SAFETY: the caller owns initialization; the typed mask excludes time.
        unsafe {
            crate::value::write::<{ Self::NUMBER }>(InhibitedCounters::programmable().bits());
        }
    }

    /// Changes one inhibit bit with CSRRS/CSRRC, preserving other counters.
    ///
    /// # Safety
    /// The counter must exist. Coordinate other firmware users, including
    /// sibling harts if the implementation shares cycle-counter control.
    pub unsafe fn set(counter: HardwareCounter, inhibited: bool) {
        let mask = InhibitedCounters::one(counter).bits();
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: the caller owns this counter; these instructions change one bit.
        unsafe {
            if inhibited {
                core::arch::asm!("csrs {csr}, {mask}", csr = const Self::NUMBER, mask = in(reg) mask, options(nostack));
            } else {
                core::arch::asm!("csrc {csr}, {mask}", csr = const Self::NUMBER, mask = in(reg) mask, options(nostack));
            }
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        {
            let _ = (mask, inhibited);
            panic!("CSR access requires RISC-V");
        }
    }
}
