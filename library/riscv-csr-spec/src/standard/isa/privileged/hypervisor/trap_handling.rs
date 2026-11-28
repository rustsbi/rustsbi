//! Hypervisor trap context and virtual interrupt state.

use crate::standard::isa::values::trap::{TrapInstructionBits, TrapValueBits};

define_csr! {
    /// Hypervisor bad trap value (`htval`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-8-hypervisor-trap-value-htval-register>.
    Htval { 0x643, r: TrapValueBits, w: TrapValueBits };

    /// Hypervisor interrupt-pending register (`hip`).
    ///
    /// Only `VSSIP`, the writable alias of `hvip.VSSIP`, permits atomic updates.
    /// Whole-register writes are withheld to protect device-driven pending bits.
    ///
    /// # Access requirements
    ///
    /// Coordinate VS software-interrupt injection with the active vCPU's owner.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#hinterruptregs>.
    Hip { 0x644, r, w(Set | Clear) } {
        VSSIP[2] { r: bool, bit }
        VSTIP[6] { r: bool }
        VSEIP[10] { r: bool }
        SGEIP[12] { r: bool }
    }

    /// Hypervisor virtual interrupt-pending register (`hvip`).
    ///
    /// Whole-register writes are permitted when the caller owns the vCPU's full
    /// software-injected interrupt-pending state.
    ///
    /// # Access requirements
    ///
    /// Coordinate the selected interrupt sources with the active vCPU's owner.
    /// AIA local interrupt injection requires the corresponding `hvien` bit.
    ///
    /// Reference: <https://docs.riscv.org/reference/aia/v1.0/VSLevel.html#5-1-3-2-virtual-interrupts-for-vs-level>.
    Hvip { 0x645, r, w(Self | Set | Clear) } {
        VSSIP[2] { rw: bool, bit }
        VSTIP[6] { rw: bool, bit }
        VSEIP[10] { rw: bool, bit }
        LCOFIP[13] { rw: bool, bit }
        #[cfg(target_pointer_width = "64")]
        LOW_RAS_EVENT[35] { rw: bool, bit }
        #[cfg(target_pointer_width = "64")]
        HIGH_RAS_EVENT[43] { rw: bool, bit }
    }

    /// Hypervisor transformed trap instruction (`htinst`).
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-9-hypervisor-trap-instruction-htinst-register>.
    Htinst { 0x64a, r: TrapInstructionBits, w: TrapInstructionBits };
}
