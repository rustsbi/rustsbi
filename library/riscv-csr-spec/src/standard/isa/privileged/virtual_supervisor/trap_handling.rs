//! Virtual-supervisor trap context and interrupt-pending state.

use crate::standard::isa::values::{
    address::Address,
    trap::{CauseValue, TrapValueBits},
};

define_csr! {
    /// Virtual supervisor scratch register (`vsscratch`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-14-virtual-supervisor-scratch-vsscratch-register>.
    Vsscratch { 0x240, r: usize, w: usize };

    /// Virtual supervisor exception program counter (`vsepc`).
    ///
    /// # Access requirements
    ///
    /// Next virtual `sret` must resume a valid guest execution context at this PC.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-15-virtual-supervisor-exception-program-counter-vsepc-register>.
    Vsepc { 0x241, r: Address, w: Address };

    /// Virtual supervisor trap cause register (`vscause`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-16-virtual-supervisor-cause-vscause-register>.
    Vscause { 0x242, r: CauseValue, w: CauseValue };

    /// Virtual supervisor trap value register (`vstval`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-17-virtual-supervisor-trap-value-vstval-register>.
    Vstval { 0x243, r: TrapValueBits, w: TrapValueBits };

    /// Virtual supervisor interrupt-pending register (`vsip`).
    ///
    /// When delegated by `hideleg`, SSIP is a writable alias of `hip.VSSIP`.
    /// STIP and SEIP are read-only views of `hip.VSTIP` and `hip.VSEIP`.
    /// These include pending sources beyond software injection through `hvip`.
    ///
    /// # Access requirements
    ///
    /// Coordinate with virtual-interrupt injection (`hvip`, `hideleg`, `hvien`).
    Vsip { 0x244, r, w(Set | Clear) } {
        SSIP[1] { r: bool, bit }
        STIP[5] { r: bool }
        SEIP[9] { r: bool }
        LCOFIP[13] { r: bool, bit }
        #[cfg(target_pointer_width = "64")]
        LOW_RAS_EVENT[35] { r: bool, bit }
        #[cfg(target_pointer_width = "64")]
        HIGH_RAS_EVENT[43] { r: bool, bit }
    }
}

interrupt_bits!(Vsip);
