//! Register identities, value representations, and access contracts.

macro_rules! impl_read_spec {
    (standard; $register:ty $(; const $index:ident: u8)?, $($value:tt)+) => {
        $crate::macros::impl_read_spec!($register $(; const $index: u8)?, $($value)+);
        impl$(<const $index: u8>)? $crate::StandardRead for $register {}
    };
    ($register:ty $(; const $index:ident: u8)?, $value:ty, $raw:ident => $decode:expr) => {
        impl$(<const $index: u8>)? $crate::ReadSpec for $register {
            type Value = $value;

            #[inline(always)]
            fn from_raw($raw: u64) -> Self::Value {
                $decode
            }
        }
    };
}
pub(crate) use impl_read_spec;

macro_rules! __csr_emit_item {
    (
        docs: [],
        $Name:ident { $num:literal $($definition:tt)* } $($tail:tt)*
    ) => {
        __csr_emit_item! {
            docs: [#[doc = concat!("CSR number: `", stringify!($num), "`.")]],
            $Name { $num $($definition)* } $($tail)*
        }
    };

    (
        docs: [$(#[doc = $doc:expr])*],
        $Name:ident {
            $num:literal, $width:expr $(, high: $hi:expr)?
            $(, r: $Read:ty)? $(, w: $Write:ty)? $(, claim: $Claim:ty)?
        };
    ) => {
        $(#[doc = $doc])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $Name;

        impl $Name {
            /// Primary architectural CSR number.
            pub const NUMBER: u16 = $num;
            $(
                /// Upper architectural CSR number on RV32.
                pub const HIGH_NUMBER: u16 = $hi;
            )?
        }

        impl $crate::sealed::Sealed for $Name {}
        impl $crate::Csr for $Name {
            const NUMBER: u16 = Self::NUMBER;
            const WIDTH: $crate::Width = $width;
        }

        $(impl_read_spec!($Name, $Read, raw => <$Read as $crate::ScalarCodec>::from_raw_u64(raw));)?
        $(__csr_write_spec!($Name, $Write);)?
        $(
            impl $crate::ClaimSpec for $Name {
                type Value = $Claim;

                #[inline(always)]
                fn from_claim_raw(raw: usize) -> Self::Value {
                    <$Claim as $crate::ScalarCodec>::from_raw_u64(raw as u64)
                }
            }
        )?
    };

    // Fielded CSR with whole-register writes.
    (
        docs: [$(#[doc = $doc:expr])*],
        $Name:ident { $num:literal, $word:ident, $width:expr $(, high: $hi:expr)?, r, w, $cap_tag:ident } { $($fields:tt)* }
    ) => {
        $(#[doc = $doc])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $Name<Mode = $crate::ReadMode>(pub(crate) $word, core::marker::PhantomData<Mode>);

        impl $Name<$crate::ReadMode> {
            /// Primary architectural CSR number.
            pub const NUMBER: u16 = $num;
            $(
                /// Upper architectural CSR number on RV32.
                pub const HIGH_NUMBER: u16 = $hi;
            )?

            __csr_emit_fields!($Name<$crate::ReadMode>, $word, $cap_tag; $($fields)*);

            /// Wraps raw register bits without modification.
            #[inline(always)]
            pub const fn from_bits(bits: $word) -> Self {
                Self(bits, core::marker::PhantomData)
            }

            /// Returns a zeroed [`WriteMode`](crate::WriteMode) register value.
            #[inline(always)]
            pub const fn zero() -> $Name<$crate::WriteMode> {
                $Name(0, core::marker::PhantomData)
            }

            /// Converts this observed snapshot into a writable value, preserving every raw bit.
            #[inline(always)]
            pub const fn into_write(self) -> $Name<$crate::WriteMode> {
                $Name(self.0, core::marker::PhantomData)
            }
        }

        impl $Name<$crate::WriteMode> {
            /// Returns a read view of the prepared value, preserving every bit.
            #[inline(always)]
            pub const fn into_read(self) -> $Name<$crate::ReadMode> {
                $Name(self.0, core::marker::PhantomData)
            }

            /// Replaces the selected field and preserves the other bits in this value.
            ///
            /// # Panics
            ///
            /// Panics if the encoded value does not fit the field.
            #[inline(always)]
            pub fn set<R, W, B, const LO: u32, const HI: u32>(
                &mut self,
                field: $crate::Field<$Name<$crate::ReadMode>, $word, R, W, B, LO, HI>,
                value: W,
            ) where
                W: $crate::WriteFieldValue,
            {
                let field_mask = field.mask();
                let encoded = value.into_field();
                assert!(encoded as u64 <= (field_mask >> LO) as u64, "value exceeds CSR field width");
                self.0 = (self.0 & !field_mask) | ((encoded as $word) << LO);
            }
        }

        impl<Mode> $Name<Mode> {
            /// Returns raw register bits without masking.
            #[inline(always)]
            pub const fn bits(self) -> $word {
                self.0
            }

            /// Decodes the selected field from this value.
            #[inline(always)]
            pub fn get<R, W, B, const LO: u32, const HI: u32>(
                self,
                field: $crate::Field<$Name<$crate::ReadMode>, $word, R, W, B, LO, HI>,
            ) -> R
            where
                R: $crate::ReadFieldValue,
            {
                R::from_field(((self.0 & field.mask()) >> LO) as usize)
            }
        }

        impl $crate::sealed::Sealed for $Name<$crate::ReadMode> {}
        impl $crate::Csr for $Name<$crate::ReadMode> {
            const NUMBER: u16 = Self::NUMBER;
            const WIDTH: $crate::Width = $width;
        }

        impl_read_spec!($Name<$crate::ReadMode>, Self, raw => Self::from_bits(raw as $word));

        __csr_write_spec!($Name<$crate::ReadMode>, $Name<$crate::WriteMode>, value =>
            $crate::WriteOp::Replace(value.bits() as u64)
        );

        __csr_emit_bit_ops!($Name, $word, $cap_tag);
    };

    // Fielded CSR limited to permitted bit operations.
    (
        docs: [$(#[doc = $doc:expr])*],
        $Name:ident { $num:literal, $word:ident, $width:expr $(, high: $hi:expr)?, r, $cap_tag:ident } { $($fields:tt)* }
    ) => {
        $(#[doc = $doc])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $Name(pub(crate) $word);

        impl $Name {
            /// Primary architectural CSR number.
            pub const NUMBER: u16 = $num;
            $(
                /// Upper architectural CSR number on RV32.
                pub const HIGH_NUMBER: u16 = $hi;
            )?

            __csr_emit_fields!($Name, $word, $cap_tag; $($fields)*);

            /// Wraps raw register bits without modification.
            #[inline(always)]
            pub const fn from_bits(bits: $word) -> Self {
                Self(bits)
            }

            /// Returns raw register bits without masking.
            #[inline(always)]
            pub const fn bits(self) -> $word {
                self.0
            }

            /// Decodes the selected field from this snapshot.
            #[inline(always)]
            pub fn get<R, W, B, const LO: u32, const HI: u32>(
                self,
                field: $crate::Field<$Name, $word, R, W, B, LO, HI>,
            ) -> R
            where
                R: $crate::ReadFieldValue,
            {
                R::from_field(((self.0 & field.mask()) >> LO) as usize)
            }
        }

        impl $crate::sealed::Sealed for $Name {}
        impl $crate::Csr for $Name {
            const NUMBER: u16 = Self::NUMBER;
            const WIDTH: $crate::Width = $width;
        }

        impl_read_spec!($Name, Self, raw => Self::from_bits(raw as $word));

        __csr_emit_bit_ops!($Name, $word, $cap_tag);
    };
}

macro_rules! define_csr {
    (
        $(#[doc = $doc:expr])*
        only32 $Name:ident { $num:literal, r, w(Set | Clear) } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, u32, $crate::Width::Rv32Only, r, set_clear } { $($fields)* }
        }
        define_csr!($($rest)*);
    };
    () => {};

    (
        $(#[doc = $doc:expr])*
        $Name:ident { $num:literal };
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, $crate::Width::Xlen };
        }
        define_csr!($($rest)*);
    };

    (
        $(#[doc = $doc:expr])*
        $Name:ident { $num:literal, claim: $Ty:ty };
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, $crate::Width::Xlen, claim: $Ty };
        }
        define_csr!($($rest)*);
    };

    // Scalar counters, RV32-only CSRs, and native-width CSRs.
    (
        $(#[doc = $doc:expr])*
        $Name:ident { ($lo:literal, $hi:literal), counter64, r: $Ty:ty $(, w: $Write:ty)? };
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $lo, $crate::Width::Counter64 { high: $hi }, high: $hi, r: $Ty $(, w: $Write)? };
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        only32 $Name:ident { $num:literal, r: $Ty:ty $(, w: $Write:ty)? };
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, $crate::Width::Rv32Only, r: $Ty $(, w: $Write)? };
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        $Name:ident { $num:literal, r: $Ty:ty $(, w: $Write:ty)? };
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, $crate::Width::Xlen, r: $Ty $(, w: $Write)? };
        }
        define_csr!($($rest)*);
    };

    (
        $(#[doc = $doc:expr])*
        $Name:ident { $num:literal, r: $Ty:ty, claim };
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, $crate::Width::Xlen, r: $Ty, claim: $Ty };
        }
        define_csr!($($rest)*);
    };

    (
        $(#[doc = $doc:expr])*
        $Name:ident { ($lo:literal, $hi:literal), timer64, r: $Rty:ty, w: $Wty:ty };
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $lo, $crate::Width::Timer64 { high: $hi }, high: $hi, r: $Rty, w: $Wty };
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        $Name:ident { ($lo:literal, $hi:expr), r: $Rty:ty, w: $Wty:ty };
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $lo, $crate::Width::Split64 { high: $hi }, high: $hi, r: $Rty, w: $Wty };
        }
        define_csr!($($rest)*);
    };

    (
        $(#[doc = $doc:expr])*
        $Name:ident { $num:literal, r: usize, w(usize | Set) };
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, $crate::Width::Xlen, r: usize, w: usize };
        }
        __csr_write_spec!($Name, $crate::Set<usize>, value => $crate::WriteOp::Set(value.0));
        define_csr!($($rest)*);
    };

    // `Self` permits whole-value writes; `Clear -> Self` returns the previous snapshot.
    (
        $(#[doc = $doc:expr])*
        only32 $Name:ident { $num:literal, r, w(Self | Set | Clear) } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, u32, $crate::Width::Rv32Only, r, w, set_clear } { $($fields)* }
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        $Name:ident { ($lo:literal, $hi:literal), r, w(Self | Set | Clear) } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $lo, u64, $crate::Width::Split64 { high: $hi }, high: $hi, r, w, set_clear } { $($fields)* }
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        only32 $Name:ident { $num:literal, r, w } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, u32, $crate::Width::Rv32Only, r, w, none } { $($fields)* }
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        $Name:ident { ($lo:literal, $hi:literal), r, w } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $lo, u64, $crate::Width::Split64 { high: $hi }, high: $hi, r, w, none } { $($fields)* }
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        $Name:ident { $num:literal, r, w } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, usize, $crate::Width::Xlen, r, w, none } { $($fields)* }
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        $Name:ident { $num:literal, r, w(Self | Set | Clear) } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, usize, $crate::Width::Xlen, r, w, set_clear } { $($fields)* }
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        $Name:ident { $num:literal, r, w(Self | Set | Clear -> Self) } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, usize, $crate::Width::Xlen, r, w, set_read_and_clear } { $($fields)* }
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        $Name:ident { $num:literal, r, w(Self | Clear) } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, usize, $crate::Width::Xlen, r, w, clear_only } { $($fields)* }
        }
        define_csr!($($rest)*);
    };

    // Fielded CSRs limited to permitted bit operations.
    (
        $(#[doc = $doc:expr])*
        $Name:ident { $num:literal, r, w(Set | Clear) } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $num, usize, $crate::Width::Xlen, r, set_clear } { $($fields)* }
        }
        define_csr!($($rest)*);
    };
    (
        $(#[doc = $doc:expr])*
        $Name:ident { ($lo:literal, $hi:literal), r, w(Set) } { $($fields:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_item! {
            docs: [$(#[doc = $doc])*],
            $Name { $lo, u64, $crate::Width::Split64 { high: $hi }, high: $hi, r, set_only } { $($fields)* }
        }
        define_csr!($($rest)*);
    };
}
