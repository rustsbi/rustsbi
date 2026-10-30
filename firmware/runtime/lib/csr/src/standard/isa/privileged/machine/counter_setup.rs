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
    CounterInhibit { 0x320, r: InhibitedCounters,
        /// Writing a complete inhibit mask stops or resumes the selected
        /// counters.
        ///
        /// # Safety
        ///
        /// Own counter initialization and coordinate every counter user,
        /// including sibling harts when the implementation shares a counter.
        unsafe w: InhibitedCounters
    }
}

impl CounterInhibit {
    /// Changes the inhibit bit for a counter.
    ///
    /// # Safety
    /// The counter must exist. Coordinate other firmware users, including
    /// sibling harts if the implementation shares cycle-counter control.
    pub unsafe fn set(counter: HardwareCounter, inhibited: bool) {
        let mask = InhibitedCounters::one(counter).bits();
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        // SAFETY: The caller coordinates changes to this counter, including
        // with sibling harts when counter control is shared.
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
