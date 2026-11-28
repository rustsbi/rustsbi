//! Supervisor status, interrupt enables, and trap vectors.

use crate::standard::isa::values::{
    status::{Endianness, SupervisorPrivilege},
    vector::{VectorBase, VectorMode},
};

define_csr! {
    /// The [supervisor view of `mstatus`](https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#sstatus).
    ///
    /// With [Ssdbltrp](https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#supv-double-trap) enabled, setting `SDT` clears `SIE`.
    /// A write can set `SIE` only if `SDT` is already clear or the same write clears it.
    ///
    /// # Access requirements
    ///
    /// Preserve the interrupted context's privilege, memory-access permissions,
    /// and extension state. Install an S-mode trap handler before enabling interrupts.
    Sstatus { 0x100, r, w(Self | Set | Clear -> Self) } {
        SIE[1] { rw: bool, bit }
        SPIE[5] { rw: bool, bit }
        UBE[6] { rw: Endianness }
        SPP[8] { rw: SupervisorPrivilege }
        SUM[18] { rw: bool }
        MXR[19] { rw: bool }
        SPELP[23] { rw: bool }
        SDT[24] { rw: bool }
    }

    /// Supervisor interrupt-enable register (`sie`).
    ///
    /// # Access requirements
    ///
    /// Install handlers before enabling their sources. The set of visible bits
    /// depends on [delegation](https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#11-1-1-3-supervisor-interrupt-sip-and-sie-registers)
    /// and [AIA virtual interrupt enables](https://docs.riscv.org/reference/aia/v1.0/MSLevel.html#virtIntrs-S).
    Sie { 0x104, r, w(Self | Set | Clear) } {
        SSIE[1] { rw: bool, bit }
        STIE[5] { rw: bool, bit }
        SEIE[9] { rw: bool, bit }
        LCOFIE[13] { rw: bool, bit }
        #[cfg(target_pointer_width = "64")]
        LOW_RAS_EVENT[35] { rw: bool, bit }
        #[cfg(target_pointer_width = "64")]
        HIGH_RAS_EVENT[43] { rw: bool, bit }
    }

    /// Supervisor trap-vector base-address register (`stvec`).
    ///
    /// # Access requirements
    ///
    /// The base must name executable S/HS trap code. In vectored mode, supply
    /// a valid entry at base + 4 × cause for every enabled interrupt.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/supervisor.html#11-1-1-2-supervisor-trap-vector-base-address-stvec-register>.
    Stvec { 0x105, r, w } {
        MODE[0..=1] { r: Result<VectorMode, usize>, w: VectorMode }
        BASE[2..=usize::BITS - 1] { rw: VectorBase }
    }
}

interrupt_bits!(Sie<Mode>);
