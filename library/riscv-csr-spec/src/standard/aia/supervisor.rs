//! Supervisor-level AIA interrupt reporting.
//!
//! Hardware access requires AIA. Coordinate interrupt-enable updates with
//! interrupt handlers on this hart.

use super::{TopExternalInterrupt, TopInterrupt};

define_csr! {
    only32 Sieh { 0x114, r, w(Self | Set | Clear) } {
        LOW_RAS_EVENT[3] { rw: bool, bit }
        HIGH_RAS_EVENT[11] { rw: bool, bit }
    }

    /// RV32 upper pending bits, subject to `mideleg`/`mvien` visibility.
    ///
    /// # Access requirements
    ///
    /// Coordinate updates with interrupt delegation and virtual-interrupt filtering.
    only32 Siph { 0x154, r, w(Set | Clear) } {
        LOW_RAS_EVENT[3] { r: bool, bit }
        HIGH_RAS_EVENT[11] { r: bool, bit }
    }

    /// Top external IMSIC interrupt.
    ///
    /// # Access requirements
    ///
    /// Use an initialized interrupt file and coordinate claims on this hart.
    /// S-mode access requires [`mvien[9] = 0`](https://docs.riscv.org/reference/aia/v1.0/MSLevel.html#virtIntrs-S)
    /// and, with [Smstateen](https://docs.riscv.org/reference/aia/v1.0/CSRs.html#CSRs-stateen),
    /// `mstateen0.IMSIC = 1`. In VS-mode the access goes to [the guest file](super::virtual_supervisor::Vstopei).
    Stopei { 0x15c, r: TopExternalInterrupt, claim };

    /// Top major interrupt report.
    Stopi { 0xdb0, r: TopInterrupt };
}
