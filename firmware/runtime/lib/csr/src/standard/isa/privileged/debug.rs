//! Debug and trace registers used to discover Sdtrig triggers.
use crate::macros::csr;

csr! {
    pub struct TriggerIndex(usize);
    pub struct TriggerData(usize);

    /// Selected debug trigger index (`tselect`).
    ///
    /// <https://github.com/riscv/riscv-debug-spec/blob/main/Sdtrig.adoc>
    TriggerSelect { 0x7a0, r: TriggerIndex,
        /// <https://github.com/riscv/riscv-debug-spec/blob/main/Sdtrig.adoc>
        ///
        /// # Safety
        ///
        /// Own the selector/data access sequence and exclude conflicting trigger
        /// accesses until the selected trigger has been read or updated.
        unsafe w: TriggerIndex
    }
    /// Type and control data for the selected debug trigger (`tdata1`).
    ///
    /// <https://github.com/riscv/riscv-debug-spec/blob/main/Sdtrig.adoc>
    TriggerData1 { 0x7a1, r: TriggerData }
}

impl From<usize> for TriggerIndex {
    fn from(index: usize) -> Self {
        Self(index)
    }
}
