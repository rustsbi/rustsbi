//! Machine trap context and interrupt-pending state.

use crate::standard::isa::values::{
    address::Address,
    trap::{CauseValue, TrapInstructionBits, TrapValueBits},
};

define_csr! {
    /// Machine scratch register (`mscratch`).
    ///
    /// # Access requirements
    ///
    /// Preserve the active M-mode trap handler scratch/stack protocol.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-13-machine-scratch-mscratch-register>.
    Mscratch { 0x340, r: usize, w: usize };

    /// Machine exception program counter (`mepc`).
    ///
    /// # Access requirements
    ///
    /// Ensure the subsequent `mret` resumes a valid execution context at this PC.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-14-machine-exception-program-counter-mepc-register>.
    Mepc { 0x341, r: Address, w: Address };

    /// Machine trap cause register (`mcause`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#mcause>.
    Mcause { 0x342, r: CauseValue, w: CauseValue };

    /// Machine trap value register (`mtval`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-16-machine-trap-value-mtval-register>.
    Mtval { 0x343, r: TrapValueBits, w: TrapValueBits };

    /// Machine interrupt-pending register (`mip`).
    ///
    /// A read of a [software-writable `SEIP`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-9-machine-interrupt-mip-and-mie-registers)
    /// includes the external interrupt signal.
    /// `CSRRS` and `CSRRC` update only the software component of `SEIP`.
    /// Writing back a previously read word could latch that external signal.
    ///
    /// # Access requirements
    ///
    /// Coordinate updates with the interrupt handler for the selected source.
    ///
    /// Guest interrupt fields follow the [H extension's aliases](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#norm:mip_mie_alias).
    Mip { 0x344, r, w(Set | Clear) } {
        SSIP[1] { r: bool, bit }
        VSSIP[2] { r: bool, bit }
        MSIP[3] { r: bool }
        /// Writable only while [`menvcfg.STCE = 0`](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#norm:menvcfg_stce_op2).
        STIP[5] { r: bool, bit }
        VSTIP[6] { r: bool }
        MTIP[7] { r: bool }
        /// Writable only while [`mvien[9] = 0`](https://docs.riscv.org/reference/aia/v1.0/MSLevel.html#virtIntrs-S).
        SEIP[9] { r: bool, bit }
        VSEIP[10] { r: bool }
        MEIP[11] { r: bool }
        SGEIP[12] { r: bool }
        LCOFIP[13] { r: bool, bit }
        #[cfg(target_pointer_width = "64")]
        LOW_RAS_EVENT[35] { r: bool, bit }
        #[cfg(target_pointer_width = "64")]
        HIGH_RAS_EVENT[43] { r: bool, bit }
    }

    /// Machine transformed trap instruction (`mtinst`, Hypervisor).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-4-5-machine-trap-instruction-mtinst-register>.
    Mtinst { 0x34a, r: TrapInstructionBits, w: TrapInstructionBits };

    /// Additional machine trap information (`mtval2`).
    ///
    /// On a guest-page fault it may hold the faulting guest physical address shifted right by two.
    /// On an S-mode double trap it holds the cause of the unexpected trap.
    /// See [guest-page faults](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-4-4-machine-second-trap-value-mtval2-register)
    /// and [double traps](https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#supv-double-trap).
    Mtval2 { 0x34b, r: TrapValueBits, w: TrapValueBits };
}

interrupt_bits!(Mip);
