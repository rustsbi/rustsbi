//! Virtual-supervisor status, interrupt enables, and trap vectors.

use crate::standard::isa::values::{
    status::{Endianness, SupervisorPrivilege},
    vector::{VectorBase, VectorMode},
};

define_csr! {
    /// Virtual supervisor status register (`vsstatus`).
    ///
    /// # Access requirements
    ///
    /// Preserve guest execution invariants used by virtual `sret`.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#vsstatus>.
    Vsstatus { 0x200, r, w(Self | Set | Clear) } {
        SIE[1] { rw: bool, bit }
        SPIE[5] { rw: bool, bit }
        UBE[6] { rw: Endianness }
        SPP[8] { rw: SupervisorPrivilege }
        SUM[18] { rw: bool }
        MXR[19] { rw: bool }
        SPELP[23] { rw: bool }
        /// Guest double-trap state. Reads as zero while `henvcfg.DTE` is clear.
        SDT[24] { rw: bool }
    }

    /// Virtual supervisor interrupt-enable register (`vsie`).
    ///
    /// # Access requirements
    ///
    /// Install VS-mode handlers for the sources enabled by the guest.
    Vsie { 0x204, r, w(Self | Set | Clear) } {
        SSIE[1] { rw: bool, bit }
        STIE[5] { rw: bool, bit }
        SEIE[9] { rw: bool, bit }
        LCOFIE[13] { rw: bool, bit }
        #[cfg(target_pointer_width = "64")]
        LOW_RAS_EVENT[35] { rw: bool, bit }
        #[cfg(target_pointer_width = "64")]
        HIGH_RAS_EVENT[43] { rw: bool, bit }
    }

    /// Virtual supervisor trap-vector base-address register (`vstvec`).
    ///
    /// # Access requirements
    ///
    /// The base is a guest virtual address. Keep the direct entry and any
    /// enabled interrupt vector entries executable in the guest address space.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#14-1-2-13-virtual-supervisor-trap-vector-base-address-vstvec-register>.
    Vstvec { 0x205, r, w } {
        MODE[0..=1] { r: Result<VectorMode, usize>, w: VectorMode }
        BASE[2..=usize::BITS - 1] { rw: VectorBase }
    }
}

interrupt_bits!(Vsie<Mode>);
