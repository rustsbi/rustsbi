//! Hypervisor status, delegation, and interrupt enables.

#[cfg(target_pointer_width = "64")]
use crate::standard::isa::values::xlen::Xlen;
use crate::standard::isa::values::{
    status::{Endianness, SupervisorPrivilege},
    trap::{ExceptionSet, InterruptSet},
};

define_csr! {
    /// Hypervisor status register (`hstatus`).
    ///
    /// # Access requirements
    ///
    /// Preserve the guest address-space and privilege context used by `sret`
    /// and hypervisor load/store instructions.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#sec:hstatus>.
    Hstatus { 0x600, r, w } {
        /// Byte order of VS-mode data accesses and implicit VS page-table accesses.
        VSBE[5] { rw: Endianness }
        /// Whether `stval` holds a guest virtual address for this trap.
        GVA[6] { rw: bool }
        /// Virtualization state saved on a trap into HS-mode.
        SPV[7] { rw: bool }
        /// Privilege saved on guest traps and used by hypervisor loads and stores.
        SPVP[8] { rw: SupervisorPrivilege }
        /// U-mode permission to execute `HLV`, `HLVX`, and `HSV`.
        HU[9] { rw: bool }
        /// Guest external interrupt number selected for `hip.VSEIP`.
        ///
        /// Zero selects no source. Implemented source numbers run from 1 through `GEILEN`.
        VGEIN[12..=17] { rw: u8 }
        /// Trap control for VS-mode `satp`, `SFENCE.VMA`, and `SINVAL.VMA` accesses.
        VTVM[20] { rw: bool }
        /// Trap control for a VS-mode `WFI` that exceeds the hart's time limit.
        VTW[21] { rw: bool }
        /// Trap control for VS-mode `SRET`.
        VTSR[22] { rw: bool }
        /// Effective VS-mode register width when `HSXLEN = 64`.
        #[cfg(target_pointer_width = "64")]
        VSXL[32..=33] { r: Result<Xlen, usize>, w: Xlen }
    }

    /// Hypervisor exception delegation (`hedeleg` / RV32 `hedelegh`).
    ///
    /// # Access requirements
    ///
    /// Install the guest trap context before delegating an exception. A set
    /// bit redirects eligible guest traps from HS to VS. Check readback for
    /// required delegations because some bit positions are read-only zero.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-2-hypervisor-trap-delegation-hedeleg-and-hideleg-registers>.
    Hedeleg { (0x602, Hedelegh::NUMBER), r: ExceptionSet, w: ExceptionSet };

    /// Upper 32 exception delegation bits of `hedeleg` on RV32.
    ///
    /// Older RV32 implementations with a 32-bit `hedeleg` have no upper CSR.
    /// Writes require the same guest trap routing guarantees as [`Hedeleg`].
    only32 Hedelegh { 0x612, r: u32, w: u32 };

    /// Hypervisor interrupt delegation register (`hideleg`).
    ///
    /// # Access requirements
    ///
    /// Ensure VS-mode handlers exist for delegated virtual interrupts.
    Hideleg { 0x603, r: InterruptSet, w: InterruptSet };

    /// Hypervisor interrupt-enable register (`hie`).
    ///
    /// # Access requirements
    ///
    /// Coordinate HS/VS interrupt handler state.
    Hie { 0x604, r: InterruptSet, w: InterruptSet };
}
