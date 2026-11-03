//! T-Head vendor CSRs.

/// T-Head machine-level CSRs.
pub mod machine {
    use crate::macros::csr;
    use crate::machine::FeatureState;

    csr! {
        /// T-Head extended status register (`mxstatus`).
        ///
        /// Xuantie OpenC910 hardware source code defines this CSR number as
        /// `0x7c0`:
        /// <https://github.com/XUANTIE-RV/openc910/blob/b91c90914c19f114d35c8f6b73408eb241ed847c/C910_RTL_FACTORY/gen_rtl/cp0/rtl/ct_cp0_iui.v#L615>.
        ExtendedStatus { 0x7c0, r }
            /// Machine acceleration execution environment (`MAEE`).
            MAEE[21] { rw: FeatureState }
    }
}
