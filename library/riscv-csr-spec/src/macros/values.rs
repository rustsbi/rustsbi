//! Register-value wrappers and field encodings.

macro_rules! csr_value {
    ($(
        $(#[doc = $doc:expr])*
        $(#[derive_ord($($ord:ident),+)])?
        $vis:vis struct $Name:ident;
    )*) => {
        $(
            csr_value! {
                @define
                $(#[doc = $doc])*
                $(#[derive_ord($($ord),+)])?
                $vis struct $Name(pub(crate) usize);
            }
        )*
    };
    (
        $(#[doc = $doc:expr])*
        $vis:vis struct $Name:ident($field:vis $word:ty);
    ) => {
        csr_value! { @define $(#[doc = $doc])* $vis struct $Name($field $word); }
    };
    (
        @define
        $(#[doc = $doc:expr])*
        $(#[derive_ord($($ord:ident),+)])?
        $vis:vis struct $Name:ident($field:vis $word:ty);
    ) => {
        $(#[doc = $doc])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq $($(, $ord)+)?)]
        $vis struct $Name($field $word);

        impl $Name {
            /// Wraps raw register bits without modification.
            #[inline(always)]
            pub const fn from_bits(bits: $word) -> Self {
                Self(bits)
            }

            /// Returns raw register bits.
            #[inline(always)]
            pub const fn bits(self) -> $word {
                self.0
            }
        }

        impl $crate::ScalarCodec for $Name {
            #[inline(always)]
            fn from_raw_u64(raw: u64) -> Self {
                Self(raw as $word)
            }

            #[inline(always)]
            fn into_raw_u64(self) -> u64 {
                self.0 as u64
            }
        }
    };
}

macro_rules! field_value {
    (
        $(#[doc = $doc:expr])*
        $vis:vis struct $Name:ident($word:ty);
    ) => {
        $(#[doc = $doc])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        $vis struct $Name($word);

        impl $Name {
            /// Wraps raw field bits without modification.
            #[inline(always)]
            pub const fn from_bits(bits: $word) -> Self {
                Self(bits)
            }

            /// Returns raw field bits.
            #[inline(always)]
            pub const fn bits(self) -> $word {
                self.0
            }
        }

        impl $crate::ReadFieldValue for $Name {
            #[inline(always)]
            fn from_field(raw: usize) -> Self {
                Self::from_bits(raw as $word)
            }
        }
        impl $crate::WriteFieldValue for $Name {
            #[inline(always)]
            fn into_field(self) -> usize {
                self.bits() as usize
            }
        }
    };
}

macro_rules! field_enum {
    (
        $(#[doc = $doc:expr])*
        $vis:vis enum $Name:ident {
            $($(#[doc = $variant_doc:expr])* $Variant:ident = $bits:literal,)*
        }
    ) => {
        $(#[doc = $doc])*
        #[repr(usize)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        $vis enum $Name {
            $(
                $(#[doc = $variant_doc])*
                $Variant = $bits,
            )*
        }

        impl $Name {
            #[inline(always)]
            pub(crate) const fn decode(bits: usize) -> Result<Self, usize> {
                match bits {
                    $($bits => Ok(Self::$Variant),)*
                    raw => Err(raw),
                }
            }

            /// Returns the architectural field encoding.
            #[inline(always)]
            pub const fn bits(self) -> usize {
                self as usize
            }
        }

        impl TryFrom<usize> for $Name {
            type Error = usize;

            #[inline(always)]
            fn try_from(raw: usize) -> Result<Self, usize> {
                Self::decode(raw)
            }
        }

        impl $crate::WriteFieldValue for $Name {
            #[inline(always)]
            fn into_field(self) -> usize {
                self.bits()
            }
        }
    };
    (
        rw
        $(#[doc = $doc:expr])*
        $vis:vis enum $Name:ident {
            $(#[doc = $zero_doc:expr])* $Zero:ident = 0,
            $(#[doc = $one_doc:expr])* $One:ident = 1,
        }
    ) => {
        field_enum! {
            $(#[doc = $doc])*
            $vis enum $Name {
                $(#[doc = $zero_doc])* $Zero = 0,
                $(#[doc = $one_doc])* $One = 1,
            }
        }

        impl $crate::ReadFieldValue for $Name {
            #[inline(always)]
            fn from_field(raw: usize) -> Self {
                if raw & 1 == 0 {
                    Self::$Zero
                } else {
                    Self::$One
                }
            }
        }

    };
}
