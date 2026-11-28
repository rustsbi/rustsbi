//! Field descriptors and their permitted bit operations.

macro_rules! __csr_field_bitcap {
    (none) => {
        $crate::capability::None
    };
    (set_only) => {
        $crate::capability::SetOnly
    };
    (clear_only) => {
        $crate::capability::ClearOnly
    };
    (set_clear) => {
        $crate::capability::SetClear
    };
    (set_read_and_clear) => {
        $crate::capability::SetClear
    };
}

macro_rules! __csr_field_type {
    ($Reg:ty, $word:ident, $cap:ident; [$bit:literal] { rw: bool, bit }) => {
        $crate::Field<$Reg, $word, bool, bool, __csr_field_bitcap!($cap), $bit, $bit>
    };
    ($Reg:ty, $word:ident, $cap:ident; [$bit:literal] { r: bool, bit }) => {
        $crate::Field<$Reg, $word, bool, (), __csr_field_bitcap!($cap), $bit, $bit>
    };
    ($Reg:ty, $word:ident, $cap:ident; [$bit:literal] { $($access:tt)+ }) => {
        __csr_field_type!($Reg, $word, $cap; [$bit..=$bit] { $($access)+ })
    };
    ($Reg:ty, $word:ident, $cap:ident; [$lo:literal..=$hi:expr] { rw: $Ty:ty }) => {
        $crate::Field<$Reg, $word, $Ty, $Ty, $crate::capability::None, $lo, { $hi }>
    };
    ($Reg:ty, $word:ident, $cap:ident; [$lo:literal..=$hi:expr] { r: $Ty:ty }) => {
        $crate::Field<$Reg, $word, $Ty, (), $crate::capability::None, $lo, { $hi }>
    };
    ($Reg:ty, $word:ident, $cap:ident; [$lo:literal..=$hi:expr] { r: $Read:ty, w: $Write:ty }) => {
        $crate::Field<$Reg, $word, $Read, $Write, $crate::capability::None, $lo, { $hi }>
    };
}

macro_rules! __csr_emit_fields {
    ($Reg:ty, $word:ident, $cap:ident;) => {};

    (
        $Reg:ty, $word:ident, $cap:ident;
        $(#[cfg($($condition:tt)*)])*
        $F:ident[$($bits:tt)+] { $($access:tt)* }
        $($rest:tt)*
    ) => {
        __csr_emit_fields! {
            $Reg, $word, $cap;
            #[doc = concat!("Bit positions: `", stringify!($($bits)+), "`.")]
            $(#[cfg($($condition)*)])*
            $F[$($bits)+] { $($access)* }
            $($rest)*
        }
    };

    (
        $Reg:ty, $word:ident, $cap:ident;
        $(#[$fmeta:meta])*
        $F:ident[$($bits:tt)+] { $($access:tt)+ }
        $($rest:tt)*
    ) => {
        $(#[$fmeta])*
        pub const $F: __csr_field_type!($Reg, $word, $cap; [$($bits)+] { $($access)+ }) =
            $crate::Field::new();
        __csr_emit_fields!($Reg, $word, $cap; $($rest)*);
    };
}
