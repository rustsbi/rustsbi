//! Machine-level AIA interrupt virtualization and reporting.
//!
//! Hardware access requires M-mode and AIA. Coordinate interrupt configuration
//! and pending-state updates with interrupt handlers on this hart.
//!
//! [`mvien` and `mvip`](https://docs.riscv.org/reference/aia/v1.0/MSLevel.html#virtIntrs-S)
//! allow M-mode to virtualize selected supervisor interrupts.

use super::{TopExternalInterrupt, TopInterrupt};

define_csr! {
    /// Virtual interrupt enables for S-mode.
    Mvien { 0x308, r, w } {
        SSIE[1] { rw: bool }
        SEIE[9] { rw: bool }
        LCOFIE[13] { rw: bool }
        #[cfg(target_pointer_width = "64")]
        LOW_RAS_EVENT[35] { rw: bool }
        #[cfg(target_pointer_width = "64")]
        HIGH_RAS_EVENT[43] { rw: bool }
    }

    /// Pending virtual interrupts at S-level.
    Mvip { 0x309, r, w } {
        SSIP[1] { rw: bool }
        /// Supervisor timer interrupt.
        STIP[5] { rw: bool }
        SEIP[9] { rw: bool }
        LCOFIP[13] { rw: bool }
        #[cfg(target_pointer_width = "64")]
        LOW_RAS_EVENT[35] { rw: bool }
        #[cfg(target_pointer_width = "64")]
        HIGH_RAS_EVENT[43] { rw: bool }
    }

    /// Upper interrupt delegation bits on RV32.
    ///
    /// # Access requirements
    ///
    /// Ensure target handlers exist before delegating interrupts.
    only32 Midelegh { 0x313, r: u32, w: u32 };

    only32 Mieh { 0x314, r, w(Self | Set | Clear) } {
        LOW_RAS_EVENT[3] { rw: bool, bit }
        HIGH_RAS_EVENT[11] { rw: bool, bit }
    }

    only32 Mvienh { 0x318, r: u32, w: u32 };

    only32 Mviph { 0x319, r: u32, w: u32 };

    /// Upper interrupt-pending bits on RV32.
    ///
    /// Implemented RAS events remain pending until software clears them.
    ///
    /// # Access requirements
    ///
    /// Coordinate pending updates with this hart's interrupt-delivery owner.
    only32 Miph { 0x354, r, w(Set | Clear) } {
        LOW_RAS_EVENT[3] { r: bool, bit }
        HIGH_RAS_EVENT[11] { r: bool, bit }
    }

    /// Top external IMSIC interrupt.
    ///
    /// # Access requirements
    ///
    /// Claiming requires an initialized M-level IMSIC interrupt file and
    /// coordination with interrupt delivery on this hart.
    Mtopei { 0x35c, r: TopExternalInterrupt, claim };

    /// Top major interrupt report.
    Mtopi { 0xfb0, r: TopInterrupt };
}

interrupt_bits!(Mvien<Mode>, Mvip<Mode>);
