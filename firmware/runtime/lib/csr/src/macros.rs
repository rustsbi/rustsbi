//! Register access and field codecs.
//! Access declarations carry the safety contracts for generated hardware operations.

macro_rules! csr {
    () => {};
    ($(#[$attr:meta])* pub struct $value:ident(usize); $($rest:tt)*) => {
        $crate::macros::csr!(@word [$(#[$attr])*] $value);
        $crate::macros::csr!($($rest)*);
    };
    (
        $(#[$attr:meta])* $name:ident { $number:literal, $($access:tt)+ }
        $(#[$field_attr:meta])* $field:ident [$first:literal $(..= $last:literal)?] { $($codec:tt)* }
        $($rest:tt)*
    ) => {
        pastey::paste! {
            $crate::macros::csr!(@values [$(#[$attr])*] [<$name ReadValue>], [<$name WriteValue>]);
            $crate::macros::csr!(@register [$(#[$attr])*] $name, $number);
            $crate::macros::csr!(@access [$(#[$attr])*] $name,
                [<$name ReadValue>], [<$name WriteValue>]; $($access)+);
            $crate::macros::csr!(@fields [$(#[$attr])*] [<$name ReadValue>], [<$name WriteValue>];
                $(#[$field_attr])* $field [$first $(..= $last)?] { $($codec)* } $($rest)*);
        }
    };
    ($(#[$attr:meta])* $name:ident { $number:literal, r: $read:ty $(, $($write:tt)+)? } $($rest:tt)*) => {
        $crate::macros::csr!(@register [$(#[$attr])*] $name, $number);
        $crate::macros::csr!(@read [$(#[$attr])*] $name, $read);
        $crate::macros::csr!(@scalar_write [$(#[$attr])*] $name, $read; $($($write)+)?);
        $crate::macros::csr!($($rest)*);
    };
    (@values [$($attr:tt)*] $read:ident, $write:ident) => {
        $crate::macros::csr!(@word [$($attr)*] $read);
        $crate::macros::csr!(@word [$($attr)*] $write);
        $($attr)*
        impl From<$write> for $read {
            fn from(value: $write) -> Self { Self(value.0) }
        }
        $($attr)*
        impl $read {
            #[inline]
            pub fn read<F: $crate::value::ReadField<Self>>(self) -> F::Value { F::read(self) }
        }
        $($attr)*
        impl $write {
            #[inline]
            pub fn read<F: $crate::value::ReadField<$read>>(self) -> F::Value { F::read($read(self.0)) }
            #[inline]
            pub fn write<F: $crate::value::WriteField<Self>>(&mut self, value: F::Value) {
                F::write(self, value);
            }
        }
    };
    (@word [$($attr:tt)*] $value:ident) => {
        $($attr)*
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $value(pub(crate) usize);
        $($attr)*
        impl $crate::value::Value for $value {
            const BITS: u32 = usize::BITS;
            fn from_bits(bits: usize) -> Self { Self(bits) }
            fn bits(self) -> usize { self.0 }
        }
        $($attr)*
        impl From<$value> for usize {
            fn from(value: $value) -> Self { value.0 }
        }
    };
    (@register [$($attr:tt)*] $name:ident, $number:literal) => {
        $($attr)* pub enum $name {}
        $($attr)* impl $name { pub const NUMBER: u16 = $number; }
        $($attr)* const _: () = assert!($number < 0x1000, "CSR number must fit in 12 bits");
    };
    (@read [$($attr:tt)*] $name:ident, $read:ty) => {
        $crate::macros::csr!(@read_value [$($attr)*] $name, $read);
        $($attr)* impl $name {
            /// Reads the current hart's register.
            ///
            /// Unavailable access traps.
            #[inline]
            pub fn read() -> $read {
                const { assert!(<$read as $crate::value::Value>::BITS <= usize::BITS); }
                // SAFETY: The declaration permits reads without caller obligations.
                <$read as $crate::value::Value>::from_bits(unsafe {
                    $crate::value::read::<{ Self::NUMBER }>()
                })
            }
        }
    };
    (@access [$($attr:tt)*] $name:ident, $read:ident, $write:ident; r) => {
        $crate::macros::csr!(@read [$($attr)*] $name, $read);
    };
    (@access [$($attr:tt)*] $name:ident, $read:ident, $write:ident; r, $(#[$write_attr:meta])* unsafe w) => {
        $crate::macros::csr!(@read [$($attr)*] $name, $read);
        $crate::macros::csr!(@write [$($attr)*] [$(#[$write_attr])*] $name, $write; [unsafe]);
        $crate::macros::csr!(@modify [$($attr)*] $name, $read, $write);
    };
    (@scalar_write [$($attr:tt)*] $name:ident, $read:ty;) => {};
    (@scalar_write [$($attr:tt)*] $name:ident, $read:ty; w: $write:ty) => {
        $crate::macros::csr!(@write [$($attr)*] [] $name, $write; []);
    };
    (@scalar_write [$($attr:tt)*] $name:ident, $read:ty; $(#[$write_attr:meta])* unsafe w: $write:ty) => {
        $crate::macros::csr!(@write [$($attr)*] [$(#[$write_attr])*] $name, $write; [unsafe]);
    };
    (@write [$($attr:tt)*] [$($method_attr:tt)*] $name:ident, $write:ty; [$($safety:tt)*]) => {
        $crate::macros::csr!(@write_value [$($attr)*] $name, $write);
        $($attr)* impl $name {
            /// Writes a complete value to the current hart's register.
            $($method_attr)*
            #[inline]
            pub $($safety)* fn write(value: $write) {
                // SAFETY: The declaration permits safe access or requires the
                // caller to establish this operation's documented contract.
                unsafe { $crate::value::write::<{ Self::NUMBER }>(
                    <$write as $crate::value::Value>::bits(value)) }
            }
        }
    };
    (@read_value [$($attr:tt)*] $name:ident, $read:ty) => {
        $($attr)* impl $crate::Readable for $name {
            const NUMBER: u16 = Self::NUMBER;
            type Value = $read;
            #[inline]
            fn decode(bits: usize) -> Self::Value {
                <$read as $crate::value::Value>::from_bits(bits)
            }
        }
    };
    (@write_value [$($attr:tt)*] $name:ident, $write:ty) => {
        $($attr)* const _: () = assert!($name::NUMBER & 0xc00 != 0xc00, "read-only CSR encoding");
        $($attr)* impl $crate::Writeable for $name {
            const NUMBER: u16 = Self::NUMBER;
            type Value = $write;
            #[inline]
            fn encode(value: Self::Value) -> usize {
                <$write as $crate::value::Value>::bits(value)
            }
        }
    };
    (@modify [$($attr:tt)*] $name:ident, $read:ty, $write:ty) => {
        $($attr)* impl $name {
            /// Updates the register through a non-atomic read/modify/write sequence.
            ///
            /// # Safety
            ///
            /// The edited value must satisfy the requirements of this register's
            /// [write](Self::write) operation.
            #[inline]
            pub unsafe fn modify(edit: impl FnOnce(&$read, &mut $write)) {
                let read = Self::read();
                let mut write = <$write as $crate::value::Value>::from_bits(
                    <$read as $crate::value::Value>::bits(read));
                edit(&read, &mut write);
                // SAFETY: The caller establishes Self::write's requirements.
                unsafe { Self::write(write) }
            }
        }
    };
    (@fields [$($attr:tt)*] $read:ident, $write:ident;
        $(#[$field_attr:meta])* $field:ident [$first:literal $(..= $last:literal)?] { $($codec:tt)* }
        $($rest:tt)*
    ) => {
        $($attr)* $(#[$field_attr])* pub enum $field {}
        $($attr)* $(#[$field_attr])* impl $field {
            pub const MASK: usize = {
                let last: u32 = $crate::macros::csr!(@last $first $(, $last)?);
                assert!(last.checked_sub($first).is_some() && last < usize::BITS);
                (usize::MAX >> (usize::BITS - 1 - last + $first)) << $first
            };
        }
        $crate::macros::csr!(@codec [$($attr)* $(#[$field_attr])*] $read, $write, $field, $first; $($codec)*);
        $crate::macros::csr!(@fields [$($attr)*] $read, $write; $($rest)*);
    };
    (@fields [$($attr:tt)*] $read:ident, $write:ident; $($rest:tt)*) => {
        $crate::macros::csr!($($rest)*);
    };
    (@last $first:literal, $last:literal) => { $last };
    (@last $first:literal) => { $first };
    (@codec [$($attr:tt)*] $read:ident, $write:ident, $field:ident, $first:literal;
        r: $value:ident { $($variant:ident = $encoding:literal),+ $(,)? } $(, $($rest:tt)*)?
    ) => {
        $($attr)* #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $value { $($variant,)+ Unknown(usize) }
        $($attr)* const _: () = {
            $(assert!(($field::MASK >> $first).checked_sub($encoding).is_some(), "field encoding exceeds its width");)+
        };
        $($attr)* impl $crate::value::Value for $value {
            const BITS: u32 = usize::BITS;
            fn from_bits(bits: usize) -> Self {
                match bits { $($encoding => Self::$variant,)+ raw => Self::Unknown(raw) }
            }
            fn bits(self) -> usize {
                match self { $(Self::$variant => $encoding,)+ Self::Unknown(raw) => raw }
            }
        }
        $crate::macros::csr!(@codec [$($attr)*] $read, $write, $field, $first; r: $value $(, $($rest)*)?);
    };
    (@codec [$($attr:tt)*] $read:ident, $write:ident, $field:ident, $first:literal;
        $direction:ident: $value:ident { $($variant:ident = $encoding:literal),+ $(,)? } $(, $($rest:tt)*)?
    ) => {
        $($attr)* #[repr(usize)] #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $value { $($variant = $encoding),+ }
        $($attr)* const _: () = {
            $(assert!(($field::MASK >> $first).checked_sub($encoding).is_some(), "field encoding exceeds its width");)+
        };
        $($attr)* impl $crate::value::Value for $value {
            const BITS: u32 = usize::BITS;
            fn from_bits(bits: usize) -> Self {
                match bits { $($encoding => Self::$variant,)+ _ => unreachable!("non-exhaustive field codec") }
            }
            fn bits(self) -> usize { self as usize }
        }
        $crate::macros::csr!(@exhaustive [$($attr)*] $field, $first; $direction; $($encoding),+);
        $crate::macros::csr!(@codec [$($attr)*] $read, $write, $field, $first; $direction: $value $(, $($rest)*)?);
    };
    (@exhaustive [$($attr:tt)*] $field:ident, $first:literal; rw; $($encoding:literal),+) => {
        $($attr)* const _: () = {
            let encodings = [$($encoding),+];
            assert!(encodings.len() == ($field::MASK >> $first) + 1, "rw enum must cover every read encoding; use separate r and w types");
        };
    };
    (@exhaustive [$($attr:tt)*] $field:ident, $first:literal; w; $($encoding:literal),+) => {};
    (@codec [$($attr:tt)*] $read:ident, $write:ident, $field:ident, $first:literal; rw: $value:ty) => {
        $crate::macros::csr!(@codec [$($attr)*] $read, $write, $field, $first; r: $value, w: $value);
    };
    (@codec [$($attr:tt)*] $read:ident, $write:ident, $field:ident, $first:literal;
        $direction:ident: $value:ty, $($rest:tt)+
    ) => {
        $crate::macros::csr!(@codec [$($attr)*] $read, $write, $field, $first; $direction: $value);
        $crate::macros::csr!(@codec [$($attr)*] $read, $write, $field, $first; $($rest)+);
    };
    (@codec [$($attr:tt)*] $read:ident, $write:ident, $field:ident, $first:literal;) => {};
    (@codec [$($attr:tt)*] $read:ident, $write:ident, $field:ident, $first:literal; r: $value:ty) => {
        $($attr)* impl $crate::value::ReadField<$read> for $field {
            type Value = $value;
            #[inline]
            fn read(register: $read) -> Self::Value {
                <$value as $crate::value::Value>::from_bits((register.0 & Self::MASK) >> $first)
            }
        }
    };
    (@codec [$($attr:tt)*] $read:ident, $write:ident, $field:ident, $first:literal; w: $value:ty) => {
        $($attr)* impl $crate::value::WriteField<$write> for $field {
            type Value = $value;
            #[inline]
            fn write(register: &mut $write, value: Self::Value) {
                let bits = <$value as $crate::value::Value>::bits(value);
                assert!(bits <= Self::MASK >> $first, "field value exceeds its declared width");
                register.0 = (register.0 & !Self::MASK) | (bits << $first);
            }
        }
    };
}

pub(crate) use csr;
