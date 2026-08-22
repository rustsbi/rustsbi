/// Implements M-, S-, and VS-mode accessors for an indirect register.
///
/// Passing a selector function and its `index` argument generates indexed
/// accessors for an indirect register array.
macro_rules! impl_ind_accessors {
    (
        $select:ident($index:ident), $ty:ident, $name:literal,
        $($mode:ident => ($doc:literal, $safety:literal)),+ $(,)?
    ) => {
        impl_ind_accessors! {
            @modes $select, $ty, $name, (indexed $index),
            $($mode => ($doc, $safety)),+
        }
    };
    (
        $select:ident, $ty:ident, $name:literal,
        $($mode:ident => ($doc:literal, $safety:literal)),+ $(,)?
    ) => {
        impl_ind_accessors! {
            @modes $select, $ty, $name, (scalar),
            $($mode => ($doc, $safety)),+
        }
    };
    (
        @modes $select:ident, $ty:ident, $name:literal, $kind:tt,
        $($mode:ident => ($doc:literal, $safety:literal)),+ $(,)?
    ) => {
        $(
            #[doc = $doc]
            pub mod $mode {
                use super::super::$mode::{read_ind, write_ind};
                use super::{$select, $ty};

                impl_ind_accessors!(@functions $select, $ty, $name, $kind, $safety);
            }
        )+
    };
    (
        @functions $select:ident, $ty:ident, $name:literal, (scalar), $safety:literal
    ) => {
        #[doc = concat!("Reads the ", $name, " register.")]
        pub fn read() -> $ty {
            let bits = unsafe { read_ind($select) };
            $ty::from_bits(bits)
        }

        #[doc = concat!("Writes the ", $name, " register.")]
        ///
        /// # Safety
        ///
        #[doc = $safety]
        pub unsafe fn write(value: $ty) {
            unsafe { write_ind($select, value.bits()) }
        }
    };
    (
        @functions $select:ident, $ty:ident, $name:literal,
        (indexed $index:ident), $safety:literal
    ) => {
        #[doc = concat!("Reads the ", $name, " register.")]
        ///
        #[doc = concat!(
            "`", stringify!($index), "` must be less than 64. On RV64, `",
            stringify!($index), "` must also be even because odd-numbered ",
            "registers do not exist."
        )]
        ///
        /// # Panics
        ///
        #[doc = concat!(
            "Panics if `", stringify!($index), "` does not identify a register ",
            "in this array for the current XLEN."
        )]
        pub fn read($index: usize) -> $ty {
            let bits = unsafe { read_ind($select($index)) };
            $ty::from_bits(bits)
        }

        #[doc = concat!("Writes the ", $name, " register.")]
        ///
        #[doc = concat!(
            "`", stringify!($index), "` must be less than 64. On RV64, `",
            stringify!($index), "` must also be even because odd-numbered ",
            "registers do not exist."
        )]
        ///
        /// # Panics
        ///
        #[doc = concat!(
            "Panics if `", stringify!($index), "` does not identify a register ",
            "in this array for the current XLEN."
        )]
        ///
        /// # Safety
        ///
        #[doc = $safety]
        pub unsafe fn write($index: usize, value: $ty) {
            unsafe { write_ind($select($index), value.bits()) }
        }
    };
}
