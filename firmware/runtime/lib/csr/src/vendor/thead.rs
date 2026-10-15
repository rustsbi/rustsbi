//! T-Head vendor CSRs.

/// T-Head machine-level CSRs.
pub mod machine {
    use crate::macros::csr;

    csr! {
        pub struct ExtendedStatusValue(usize);

        /// T-Head extended status register (`mxstatus`).
        ///
        /// See
        /// <https://github.com/riscv-software-src/opensbi/blob/master/platform/generic/include/thead/c9xx_encoding.h>.
        ExtendedStatus { 0x7c0, r: ExtendedStatusValue }
    }
}
