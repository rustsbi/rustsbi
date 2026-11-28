//! RISC-V [Debug Trigger (`Sdtrig` v1.0)](https://docs.riscv.org/reference/debug/v1.0/Sdtrig.html) register definitions.
//!
//! Write the desired index to `tselect` and verify it reads back before accessing
//! the [trigger data](https://docs.riscv.org/reference/debug/v1.0/Sdtrig.html#csr-tdata1)
//! or `tinfo`. Serialize the sequence against changes of `tselect`
//! on this hart. A trigger with `tdata1.dmode=1` can only be modified in Debug Mode.

csr_value! {
    /// Debug trigger control data word (`tdata1`).
    pub struct TriggerData;
}

impl TriggerData {
    /// Extracts the 4-bit `type` field (`tdata1[XLEN-1:XLEN-4]`).
    #[inline(always)]
    pub const fn trigger_type(self) -> u8 {
        ((self.0 >> (usize::BITS - 4)) & 0xf) as u8
    }

    /// Reports whether trigger modification is restricted to Debug Mode.
    #[inline(always)]
    pub const fn dmode(self) -> bool {
        ((self.0 >> (usize::BITS - 5)) & 1) != 0
    }
}

define_csr! {
    /// Debug trigger selector (`tselect`).
    ///
    /// # Access requirements
    ///
    /// Own the `tselect`/`tdata*` selection sequence and exclude conflicting
    /// trigger accesses until the selected trigger operation completes.
    ///
    /// Reference: <https://docs.riscv.org/reference/debug/v1.0/Sdtrig.html#csr-tselect>.
    Tselect { 0x7a0, r: usize, w: usize };

    Tdata1 { 0x7a1, r: TriggerData, w: TriggerData };

    Tdata2 { 0x7a2, r: usize, w: usize };

    Tdata3 { 0x7a3, r: usize, w: usize };

    Tinfo { 0x7a4, r: usize };

    /// Debug trigger control (`tcontrol`).
    ///
    /// # Access requirements
    ///
    /// Execute in M-mode while coordinating trigger enable state.
    Tcontrol { 0x7a5, r: usize, w: usize };
}
