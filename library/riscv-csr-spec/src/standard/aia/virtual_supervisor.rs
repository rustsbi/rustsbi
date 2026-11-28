//! Virtual-supervisor AIA interrupt reporting.
//!
//! Hardware access requires AIA and the H extension.

use super::{TopExternalInterrupt, TopInterrupt};

define_csr! {
    only32 Vsieh { 0x214, r, w(Self | Set | Clear) } {
        LOW_RAS_EVENT[3] { rw: bool, bit }
        HIGH_RAS_EVENT[11] { rw: bool, bit }
    }

    /// RV32 upper virtual pending bits, subject to `hideleg`/`hvien` visibility.
    ///
    /// # Access requirements
    ///
    /// Coordinate updates with guest interrupt delegation and injection.
    only32 Vsiph { 0x254, r, w(Set | Clear) } {
        LOW_RAS_EVENT[3] { r: bool, bit }
        HIGH_RAS_EVENT[11] { r: bool, bit }
    }

    /// Top external IMSIC interrupt.
    ///
    /// # Access requirements
    ///
    /// Ensure [`hstatus.VGEIN` selects a guest interrupt file](https://docs.riscv.org/reference/aia/v1.0/CSRs.html#hypervisor-vs-csrs)
    /// and coordinate claims on this hart. With [Smstateen](https://docs.riscv.org/reference/aia/v1.0/CSRs.html#CSRs-stateen),
    /// HS-mode access requires `mstateen0.IMSIC = 1`. M-mode bypasses these controls.
    /// A guest accesses this file through `Stopei`, requiring both
    /// `mstateen0.IMSIC` and `hstateen0.IMSIC` when implemented.
    /// Invalid `VGEIN` or insufficient state enables trap.
    Vstopei { 0x25c, r: TopExternalInterrupt, claim };

    /// Top major interrupt report.
    Vstopi { 0xeb0, r: TopInterrupt };
}
