//! T-Head vendor CSRs.

/// T-Head machine-level CSRs.
pub mod machine {
    use crate::macros::csr;
    use crate::machine::FeatureState;

    csr! {
        /// T-Head extended status register (`mxstatus`).
        ///
        /// The XuanTie OpenC906 User Manual, section 15.1.7.1, describes this
        /// register: <https://cs107e.github.io/readings/XuanTie-OpenC906-UserManual.pdf>.
        /// OpenSBI identifies its C9xx CSR number as `0x7c0`:
        /// <https://github.com/riscv-software-src/opensbi/blob/master/platform/generic/include/thead/c9xx_encoding.h>.
        ExtendedStatus { 0x7c0, r }
            /// Machine acceleration execution environment (`MAEE`).
            ///
            /// OpenSBI names this field `THEAD_MXSTATUS_MAEE`:
            /// <https://github.com/riscv-software-src/opensbi/blob/master/platform/generic/include/thead/c9xx_encoding.h>.
            MAEE[21] { rw: FeatureState }
    }
}
