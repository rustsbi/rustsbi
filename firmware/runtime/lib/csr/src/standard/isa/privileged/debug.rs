//! Debug and trace registers used to discover Sdtrig triggers.
//!
//! See <https://docs.riscv.org/reference/debug/Sdtrig.html>.
use crate::macros::csr;

csr! {
    pub struct TriggerIndex(usize);
    pub struct TriggerData(usize);

    /// Debug trigger selector (`tselect`).
    ///
    /// See <https://github.com/riscv/riscv-debug-spec/blob/main/Sdtrig.adoc>.
    TriggerSelect { 0x7a0, r: TriggerIndex,
        /// See
        /// <https://github.com/riscv/riscv-debug-spec/blob/main/Sdtrig.adoc>.
        ///
        /// # Safety
        ///
        /// Own the selector/data access sequence and exclude conflicting
        /// trigger accesses until the selected trigger has been read or
        /// updated.
        unsafe w: TriggerIndex
    }
    /// Debug trigger control data (`tdata1`).
    ///
    /// See <https://github.com/riscv/riscv-debug-spec/blob/main/Sdtrig.adoc>.
    TriggerData1 { 0x7a1, r: TriggerData }
}

impl From<usize> for TriggerIndex {
    #[inline]
    fn from(index: usize) -> Self {
        Self(index)
    }
}
