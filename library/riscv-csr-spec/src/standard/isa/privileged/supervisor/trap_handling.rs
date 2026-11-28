//! Supervisor trap context and interrupt-pending state.

use crate::standard::isa::values::{
    address::Address,
    trap::{CauseValue, TrapValueBits},
};

define_csr! {
    /// Supervisor scratch register (`sscratch`).
    ///
    /// # Access requirements
    ///
    /// Preserve the S-mode trap handler scratch protocol.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#11-1-1-6-supervisor-scratch-sscratch-register>.
    Sscratch { 0x140, r: usize, w: usize };

    /// Supervisor exception program counter (`sepc`).
    ///
    /// # Access requirements
    ///
    /// Ensure the subsequent `sret` resumes a valid execution context at this PC.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#11-1-1-7-supervisor-exception-program-counter-sepc-register>.
    Sepc { 0x141, r: Address, w: Address };

    /// Supervisor trap cause register (`scause`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#scause>.
    Scause { 0x142, r: CauseValue, w: CauseValue };

    /// Supervisor trap value register (`stval`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#11-1-1-9-supervisor-trap-value-stval-register>.
    Stval { 0x143, r: TrapValueBits, w: TrapValueBits };

    /// Supervisor interrupt-pending register (`sip`).
    ///
    /// [`STIP` is read-only](https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#11-1-1-3-supervisor-interrupt-sip-and-sie-registers).
    /// `SEIP` becomes writable through the [AIA `mvip` alias](https://docs.riscv.org/reference/aia/v1.0/MSLevel.html#virtIntrs-S)
    /// when `mideleg[9] = 0` and `mvien[9] = 1`.
    ///
    /// # Access requirements
    ///
    /// Coordinate pending updates with interrupt delegation and AIA filtering (`mideleg`/`mvien`).
    Sip { 0x144, r, w(Set | Clear) } {
        SSIP[1] { r: bool, bit }
        STIP[5] { r: bool }
        /// Writable through the AIA virtual interrupt alias.
        SEIP[9] { r: bool, bit }
        LCOFIP[13] { r: bool, bit }
        #[cfg(target_pointer_width = "64")]
        LOW_RAS_EVENT[35] { r: bool, bit }
        #[cfg(target_pointer_width = "64")]
        HIGH_RAS_EVENT[43] { r: bool, bit }
    }
}

interrupt_bits!(Sip);
