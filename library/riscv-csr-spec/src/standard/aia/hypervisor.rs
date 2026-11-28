//! Hypervisor AIA interrupt virtualization and priority controls.
//!
//! Hardware access requires AIA and the H extension.
//! Coordinate changes to interrupt routing and priorities with the active guest.
//! The `hviprio*` registers pack VS interrupt priorities into bytes.
//! For example, bits 15:8 and 31:24 of `hviprio1` specify the supervisor
//! software and timer priorities. Preserve the other bytes when updating one.
//!
//! See the [AIA CSRs](https://docs.riscv.org/reference/aia/v1.0/CSRs.html)
//! and [VS priority fields](https://docs.riscv.org/reference/aia/v1.0/VSLevel.html#5-1-3-1-configuring-priorities-of-major-interrupts-at-vs-level).

use super::MajorIid;

define_csr! {
    /// Virtual interrupt enables for VS-mode.
    Hvien { 0x608, r, w } {
        /// Local counter overflow interrupt.
        LCOFIE[13] { rw: bool }
        #[cfg(target_pointer_width = "64")]
        LOW_RAS_EVENT[35] { rw: bool }
        #[cfg(target_pointer_width = "64")]
        HIGH_RAS_EVENT[43] { rw: bool }
    }

    /// Injected VS interrupt priority and control.
    ///
    /// Reference: <https://docs.riscv.org/reference/aia/v1.0/VSLevel.html#5-1-3-2-virtual-interrupts-for-vs-level>.
    Hvictl { 0x609, r, w } {
        /// Virtual interrupt priority.
        IPRIO[0..=7] { rw: u8 }
        /// Interrupt priority mode.
        IPRIOM[8] { rw: bool }
        /// Default Priority Rank.
        DPR[9] { rw: bool }
        /// Major interrupt identity.
        IID[16..=27] { rw: MajorIid }
        /// Virtual Trap Interrupt.
        VTI[30] { rw: bool }
    }

    only32 Hidelegh { 0x613, r: u32, w: u32 };

    only32 Hvienh { 0x618, r: u32, w: u32 };

    Hviprio1 { 0x646, r: usize, w: usize };

    Hviprio2 { 0x647, r: usize, w: usize };

    only32 Hviph { 0x655, r, w(Self | Set | Clear) } {
        LOW_RAS_EVENT[3] { rw: bool, bit }
        HIGH_RAS_EVENT[11] { rw: bool, bit }
    }

    only32 Hviprio1h { 0x656, r: u32, w: u32 };

    only32 Hviprio2h { 0x657, r: u32, w: u32 };
}
