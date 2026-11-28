//! Const-indexed CSR identities using the common access contracts.

macro_rules! define_indexed_csr {
    (
        $(#[doc = $doc:expr])*
        $Name:ident<$Index:ident> {
            index: $first:literal..=$last:literal,
            base: $base:literal,
            $(high: $high:literal,)?
            width: $width:expr,
            r: $Read:ty $(, w: $Write:ty)?
        }
    ) => {
        $(#[doc = $doc])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $Name<const $Index: u8>;

        impl<const $Index: u8> $Name<$Index> {
            /// The CSR number for this index.
            ///
            /// An out-of-range index fails constant evaluation.
            pub const NUMBER: u16 = {
                assert!(matches!($Index, $first..=$last), "invalid CSR index");
                $base + $Index as u16
            };
            $(
                /// Upper architectural CSR number on RV32.
                pub const HIGH_NUMBER: u16 = {
                    assert!(matches!($Index, $first..=$last), "invalid CSR index");
                    $high + $Index as u16
                };
            )?
        }

        impl<const $Index: u8> $crate::sealed::Sealed for $Name<$Index> {}
        impl<const $Index: u8> $crate::Csr for $Name<$Index> {
            const NUMBER: u16 = Self::NUMBER;
            const WIDTH: $crate::Width = $width;
        }
        impl_read_spec!($Name<$Index>; const $Index: u8, $Read, raw =>
            <$Read as $crate::ScalarCodec>::from_raw_u64(raw));
        $(
            __csr_write_spec!(const $Index: u8; $Name<$Index>, $Write);
        )?
    };
}
