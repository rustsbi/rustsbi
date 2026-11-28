//! Machine status, delegation, interrupt enables, and trap vectors.

use crate::standard::isa::values::{
    status::{Endianness, Privilege, SupervisorPrivilege},
    trap::{ExceptionSet, InterruptSet},
    vector::{VectorBase, VectorMode},
    xlen::Xlen,
};

define_csr! {
    /// Machine status register (`mstatus`).
    ///
    /// With [Smdbltrp](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#machine-double-trap), writing `MDT = 1` clears `MIE`.
    /// Setting `MIE` requires `MDT = 0`, either beforehand or in the same RV64 write.
    /// On RV32 the `MDT` field lives in [`Mstatush`].
    ///
    /// # Access requirements
    ///
    /// Whole-register writes and interrupt-bit updates must execute in M-mode
    /// on the current hart while preserving address-space, extension-state,
    /// and interrupt-exclusion invariants (`MPRV` changes effective load/store privilege).
    Mstatus { 0x300, r, w(Self | Set | Clear -> Self) } {
        SIE[1] { rw: bool, bit }
        /// Global machine interrupt enable.
        MIE[3] { rw: bool, bit }
        SPIE[5] { rw: bool, bit }
        UBE[6] { rw: Endianness }
        /// Machine interrupt enable saved on trap entry.
        MPIE[7] { rw: bool, bit }
        SPP[8] { rw: SupervisorPrivilege }
        /// Previous privilege mode saved on entry to M-mode. With H, `MPV` records virtualization.
        MPP[11..=12] { r: Result<Privilege, usize>, w: Privilege }
        /// Use the privilege in `MPP` for explicit loads and stores.
        MPRV[17] { rw: bool }
        SUM[18] { rw: bool }
        MXR[19] { rw: bool }
        SPELP[23] { rw: bool }
        /// S-mode double-trap state, also visible through `sstatus.SDT`.
        SDT[24] { rw: bool }
        /// Effective U-mode register width.
        #[cfg(target_pointer_width = "64")]
        UXL[32..=33] { r: Result<Xlen, usize>, w: Xlen }
        /// Effective S-mode register width.
        #[cfg(target_pointer_width = "64")]
        SXL[34..=35] { r: Result<Xlen, usize>, w: Xlen }
        /// S-mode data endianness.
        #[cfg(target_pointer_width = "64")]
        SBE[36] { rw: Endianness }
        /// Whether `mtval` holds a guest virtual address for this trap.
        #[cfg(target_pointer_width = "64")]
        GVA[38] { rw: bool }
        /// Virtualization state saved on trap entry.
        #[cfg(target_pointer_width = "64")]
        MPV[39] { rw: bool }
        /// Expected landing-pad state saved on trap entry by `Zicfilp`.
        #[cfg(target_pointer_width = "64")]
        MPELP[41] { rw: bool }
        /// Machine double-trap control.
        #[cfg(target_pointer_width = "64")]
        MDT[42] { rw: bool }
    }

    /// Additional machine status register on RV32 (`mstatush`).
    ///
    /// # Access requirements
    ///
    /// Execute only on RV32 in M-mode while preserving endianness, virtualization,
    /// and double-trap invariants on the current hart.
    only32 Mstatush { 0x310, r, w } {
        SBE[4] { rw: Endianness }
        GVA[6] { rw: bool }
        MPV[7] { rw: bool }
        MPELP[9] { rw: bool }
        MDT[10] { rw: bool }
    }

    /// Machine ISA register (`misa`).
    ///
    /// # Access requirements
    ///
    /// Selected extensions and instruction alignment must remain valid for all
    /// live code, register state, and trap handlers executing on this hart.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#misa>.
    Misa { 0x301, r, w } {}

    /// Machine exception delegation (`medeleg` / RV32 `medelegh`).
    ///
    /// # Access requirements
    ///
    /// Install the S/HS trap context before delegating an exception. Check
    /// readback when a delegation bit is required, as it may be read-only zero.
    Medeleg { (0x302, Medelegh::NUMBER), r: ExceptionSet, w: ExceptionSet };

    /// Upper half of `medeleg` on RV32 (`medelegh`).
    ///
    /// # Access requirements
    ///
    /// Access requires RV32 with the upper delegation CSR. Older implementations
    /// may expose only the primary word of `medeleg`.
    only32 Medelegh { 0x312, r: u32, w: u32 };

    /// Machine interrupt delegation register (`mideleg`).
    ///
    /// # Access requirements
    ///
    /// Ensure target handlers exist for every delegated interrupt source.
    Mideleg { 0x303, r: InterruptSet, w: InterruptSet };

    /// Machine interrupt-enable register (`mie`).
    ///
    /// # Access requirements
    ///
    /// Install handlers before enabling their interrupt sources.
    /// The [H extension](https://docs.riscv.org/reference/isa/v20260120/priv/hypervisor.html#norm:mip_mie_alias)
    /// adds the virtual supervisor and guest external interrupt enables to the
    /// [machine interrupt-enable register](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-9-machine-interrupt-mip-and-mie-registers).
    Mie { 0x304, r, w(Self | Set | Clear) } {
        SSIE[1] { rw: bool, bit }
        VSSIE[2] { rw: bool, bit }
        MSIE[3] { rw: bool, bit }
        STIE[5] { rw: bool, bit }
        VSTIE[6] { rw: bool, bit }
        MTIE[7] { rw: bool, bit }
        SEIE[9] { rw: bool, bit }
        VSEIE[10] { rw: bool, bit }
        MEIE[11] { rw: bool, bit }
        SGEIE[12] { rw: bool, bit }
        LCOFIE[13] { rw: bool, bit }
        #[cfg(target_pointer_width = "64")]
        LOW_RAS_EVENT[35] { rw: bool, bit }
        #[cfg(target_pointer_width = "64")]
        HIGH_RAS_EVENT[43] { rw: bool, bit }
    }

    /// Machine trap-vector base-address register (`mtvec`).
    ///
    /// # Access requirements
    ///
    /// The base must be four-byte aligned. Vectored mode can require stricter
    /// alignment and needs valid entries at base + 4 × interrupt cause.
    ///
    /// Reference: <https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#2-1-1-7-machine-trap-vector-base-address-mtvec-register>.
    Mtvec { 0x305, r, w } {
        MODE[0..=1] { r: Result<VectorMode, usize>, w: VectorMode }
        BASE[2..=usize::BITS - 1] { rw: VectorBase }
    }
}

impl<Mode> Misa<Mode> {
    /// Returns whether the snapshot has the bit for the given uppercase extension letter.
    ///
    /// Returns `false` for characters outside `'A'..='Z'`.
    #[inline(always)]
    pub const fn has_extension(self, extension: char) -> bool {
        let code = extension as u32;
        code >= 'A' as u32 && code <= 'Z' as u32 && (self.0 & (1usize << (code - 'A' as u32))) != 0
    }

    /// Decodes `MXL` from the top two bits of `misa`.
    #[inline(always)]
    pub const fn mxl(self) -> Result<Xlen, usize> {
        Xlen::decode((self.0 >> (usize::BITS - 2)) & 0b11)
    }
}

interrupt_bits!(Mie<Mode>);
