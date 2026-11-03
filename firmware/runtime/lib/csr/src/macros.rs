//! Register access and field codecs.
//! Access declarations carry the safety contracts for generated hardware
//! operations.
//! CSR instruction semantics are defined by
//! <https://docs.riscv.org/reference/isa/unpriv/zicsr.html>.
//!
//! # Register declarations
//!
//! Field-bearing declarations generate `Name<Read>` and `Name<Write>` values.
//! Their read and write field types may differ where reserved read encodings
//! are not legal write encodings.
//! Fieldless declarations without an explicit scalar type generate a plain
//! value struct named after the register; reads and writes of that register
//! use the same type.
//! Explicit scalar values use `r: Value` and `w: Value`.
//! Register types identify CSRs; hardware operations are associated functions.
//! Ordinary reads are safe; writes require their current-hart access contract.
//!
//! A `Name<Write>` value starts as zeroed local bits through `Default`.
//! This is an encoding primitive, not a hardware reset state or a universally
//! valid register configuration.
//!
//! | Declaration | Generated operation | Safety contract |
//! | --- | --- | --- |
//! | `r: Value` | Safe CSR read | Unavailable or inaccessible hardware traps |
//! | `w: Value` | Unsafe CSR write | The caller establishes current-hart access and coordinates updates |
//! | `unsafe w: Value` | Unsafe CSR write | The caller also satisfies register-specific requirements |
//! | Dedicated method | Explicit instruction sequence | The method defines effects and caller obligations |
//!
//! Execution-environment writes use `unsafe w` with register-specific safety
//! conditions.
//! Operations with side effects or indirect effects need dedicated
//! implementations.
//! Use specification names such as `claim`, `set_bit`, and `clear_bit` for these
//! operations.
//! Document their affected state, instruction sequence, ordering, and caller
//! obligations, with the applicable specification reference.
//!
//! Omit ordinary access declarations that could bypass a dedicated operation.
//! For example, `InterruptPending { 0x344, r }` omits `w`, which suppresses
//! `write` and `modify`.
//! Keep register identities and pure field codecs independent of hardware
//! access.

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
        $crate::macros::csr!(@values [$(#[$attr])*] $name);
        $crate::macros::csr!(@number [$(#[$attr])*] $name, $number);
        $crate::macros::csr!(@access [$(#[$attr])*] $name,
            $name<$crate::Read>, $name<$crate::Write>; $($access)+);
        $crate::macros::csr!(@fields [$(#[$attr])*] $name<$crate::Read>, $name<$crate::Write>;
            $(#[$field_attr])* $field [$first $(..= $last)?] { $($codec)* } $($rest)*);
    };
    ($(#[$attr:meta])* $name:ident { $number:literal, r: $read:ty $(, $($write:tt)+)? } $($rest:tt)*) => {
        $crate::macros::csr!(@register [$(#[$attr])*] $name, $number);
        $crate::macros::csr!(@read [$(#[$attr])*] $name, $read);
        $crate::macros::csr!(@scalar_write [$(#[$attr])*] $name, $read; $($($write)+)?);
        $crate::macros::csr!($($rest)*);
    };
    ($(#[$attr:meta])* $name:ident { $number:literal, r $(, $($write:tt)+)? } $($rest:tt)*) => {
        $crate::macros::csr!(@word [$(#[$attr])*] $name);
        $crate::macros::csr!(@number [$(#[$attr])*] $name, $number);
        $crate::macros::csr!(@read [$(#[$attr])*] $name, $name);
        $crate::macros::csr!(@scalar_write [$(#[$attr])*] $name, $name; $($($write)+)?);
        $crate::macros::csr!($($rest)*);
    };
    (@values [$($attr:tt)*] $name:ident) => {
        $($attr)*
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $name<Mode = $crate::Read>(pub(crate) usize, core::marker::PhantomData<Mode>);

        $($attr)* impl<Mode> $name<Mode> {
            #[inline]
            pub(crate) const fn from_raw(bits: usize) -> Self {
                Self(bits, core::marker::PhantomData)
            }
        }
        $($attr)* impl<Mode: Copy> $crate::value::Value for $name<Mode> {
            const BITS: u32 = usize::BITS;
            #[inline]
            fn from_bits(bits: usize) -> Self { Self::from_raw(bits) }
            #[inline]
            fn bits(self) -> usize { self.0 }
        }
        $($attr)* impl<Mode> From<$name<Mode>> for usize {
            #[inline]
            fn from(value: $name<Mode>) -> Self { value.0 }
        }
        $($attr)* impl From<$name<$crate::Write>> for $name<$crate::Read> {
            #[inline]
            fn from(value: $name<$crate::Write>) -> Self { Self::from_raw(value.0) }
        }
        $($attr)* impl Default for $name<$crate::Write> {
            /// Creates a prepared value with every local bit clear.
            #[inline]
            fn default() -> Self { Self::from_raw(0) }
        }
        $($attr)* impl $name<$crate::Read> {
            /// Decodes an in-memory register value without accessing hardware.
            #[inline]
            pub const fn from_bits(bits: usize) -> Self { Self::from_raw(bits) }
            /// Reads a field from this local value without accessing hardware.
            #[inline]
            pub fn get<F: $crate::value::ReadField<Self>>(self) -> F::Value { F::get(self) }
        }
        $($attr)* impl $name<$crate::Write> {
            /// Reads a field from this local value without accessing hardware.
            #[inline]
            pub fn get<F: $crate::value::ReadField<$name<$crate::Read>>>(self) -> F::Value {
                F::get($name::from_bits(self.0))
            }
            /// Writes a field in this local value without accessing hardware.
            #[inline]
            pub fn set<F: $crate::value::WriteField<Self>>(&mut self, value: F::Value) {
                F::set(self, value);
            }
        }
    };
    (@word [$($attr:tt)*] $value:ident) => {
        $($attr)*
        #[repr(transparent)]
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub struct $value(pub(crate) usize);
        $($attr)* impl $crate::value::Value for $value {
            const BITS: u32 = usize::BITS;
            #[inline]
            fn from_bits(bits: usize) -> Self { Self(bits) }
            #[inline]
            fn bits(self) -> usize { self.0 }
        }
        $($attr)* impl From<$value> for usize {
            #[inline]
            fn from(value: $value) -> Self { value.0 }
        }
    };
    (@register [$($attr:tt)*] $name:ident, $number:literal) => {
        $($attr)*
        pub enum $name {}
        $crate::macros::csr!(@number [$($attr)*] $name, $number);
    };
    (@number [$($attr:tt)*] $name:ident, $number:literal) => {
        $($attr)* impl $name {
            pub const NUMBER: u16 = $number;
        }
        $($attr)* const _: () = assert!($number < 0x1000, "CSR number must fit in 12 bits");
    };
    (@read [$($attr:tt)*] $name:ident, $read:ty) => {
        $($attr)* impl $name {
            /// Reads this hart's register. Unavailable access traps.
            #[inline]
            pub fn read() -> $read {
                const { assert!(<$read as $crate::value::Value>::BITS <= usize::BITS); }
                // SAFETY: This instruction observes the register without
                // changing execution state; inaccessible hardware traps.
                <$read as $crate::value::Value>::from_bits(unsafe {
                    $crate::value::read::<{ Self::NUMBER }>()
                })
            }
        }
    };
    (@access [$($attr:tt)*] $name:ident, $read:ty, $write:ty; r) => {
        $crate::macros::csr!(@read [$($attr)*] $name, $read);
    };
    (@access [$($attr:tt)*] $name:ident, $read:ty, $write:ty; r, $(#[$write_attr:meta])* unsafe w) => {
        $crate::macros::csr!(@access [$($attr)*] $name, $read, $write;
            r, $(#[$write_attr])* unsafe w: $write);
    };
    (@access [$($attr:tt)*] $name:ident, $read:ty, $write:ty; r, $(#[$write_attr:meta])* unsafe w: $complete:ty) => {
        $crate::macros::csr!(@read [$($attr)*] $name, $read);
        $crate::macros::csr!(@write [$($attr)*] [$(#[$write_attr])*] $name, $complete);
        $($attr)* impl $name {
            /// Edits a snapshot and writes it back in one non-atomic sequence.
            ///
            /// # Safety
            ///
            /// Execute on the intended hart in a permitted privilege mode.
            /// Coordinate conflicting accesses and satisfy this register's
            /// write requirements. Unavailable hardware requires a valid trap context.
            #[inline]
            pub unsafe fn modify(edit: impl FnOnce(&$read, &mut $write)) {
                let read = Self::read();
                let mut write = <$write as $crate::value::Value>::from_bits(
                    <$read as $crate::value::Value>::bits(read));
                edit(&read, &mut write);
                // SAFETY: These fields belong to this register, and the
                // caller establishes its hardware-write requirements.
                unsafe { $crate::value::write::<{ Self::NUMBER }>(
                    <$write as $crate::value::Value>::bits(write)) }
            }
        }
    };
    (@scalar_write [$($attr:tt)*] $name:ident, $read:ty;) => {};
    (@scalar_write [$($attr:tt)*] $name:ident, $read:ty; w: $write:ty) => {
        $crate::macros::csr!(@write [$($attr)*] [#[doc = ""] #[doc = "# Safety"]] $name, $write);
    };
    (@scalar_write [$($attr:tt)*] $name:ident, $read:ty; $(#[$write_attr:meta])* unsafe w) => {
        $crate::macros::csr!(@write [$($attr)*] [$(#[$write_attr])*] $name, $read);
    };
    (@scalar_write [$($attr:tt)*] $name:ident, $read:ty; $(#[$write_attr:meta])* unsafe w: $write:ty) => {
        $crate::macros::csr!(@write [$($attr)*] [$(#[$write_attr])*] $name, $write);
    };
    (@write [$($attr:tt)*] [$($method_attr:tt)*] $name:ident, $write:ty) => {
        $($attr)* const _: () = assert!($name::NUMBER & 0xc00 != 0xc00, "read-only CSR encoding");
        $($attr)* impl $name {
            /// Writes a complete value to this hart's register.
            $($method_attr)*
            /// Execute on the intended hart in a permitted privilege mode.
            /// Coordinate conflicting accesses. Unavailable hardware requires
            /// a valid trap context.
            #[inline]
            pub unsafe fn write(value: $write) {
                // SAFETY: The caller establishes access and write requirements.
                unsafe { $crate::value::write::<{ Self::NUMBER }>(
                    <$write as $crate::value::Value>::bits(value)) }
            }
        }
    };
    (@fields [$($attr:tt)*] $read:ty, $write:ty;
        $(#[$field_attr:meta])* $field:ident [$first:literal $(..= $last:literal)?] { $($codec:tt)* }
        $($rest:tt)*
    ) => {
        $($attr)* $(#[$field_attr])* pub enum $field {}
        $($attr)* $(#[$field_attr])* const _: () = {
            let _ = $crate::macros::csr!(@max [$first $(..= $last)?]);
        };
        $crate::macros::csr!(@codec [$($attr)* $(#[$field_attr])*] $read, $write, $field, [$first $(..= $last)?]; $($codec)*);
        $crate::macros::csr!(@fields [$($attr)*] $read, $write; $($rest)*);
    };
    (@fields [$($attr:tt)*] $read:ty, $write:ty; $($rest:tt)*) => {
        $crate::macros::csr!($($rest)*);
    };
    (@last $first:literal, $last:literal) => { $last };
    (@last $first:literal) => { $first };
    (@max [$first:literal $(..= $last:literal)?]) => {{
        let last: u32 = $crate::macros::csr!(@last $first $(, $last)?);
        assert!(last.checked_sub($first).is_some() && last < usize::BITS);
        usize::MAX >> (usize::BITS - 1 - last + $first)
    }};
    (@codec [$($attr:tt)*] $read:ty, $write:ty, $field:ident, $bounds:tt;
        r: $value:ident { $($variant:ident = $encoding:literal),+ $(,)? } $(, $($rest:tt)*)?
    ) => {
        $($attr)* #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $value { $($variant,)+ Unknown(usize) }
        $($attr)* const _: () = {
            $(assert!($crate::macros::csr!(@max $bounds).checked_sub($encoding).is_some(), "field encoding exceeds its width");)+
        };
        $($attr)* impl $crate::value::Value for $value {
            const BITS: u32 = usize::BITS;
            #[inline]
            fn from_bits(bits: usize) -> Self {
                match bits { $($encoding => Self::$variant,)+ raw => Self::Unknown(raw) }
            }
            #[inline]
            fn bits(self) -> usize {
                match self { $(Self::$variant => $encoding,)+ Self::Unknown(raw) => raw }
            }
        }
        $crate::macros::csr!(@codec [$($attr)*] $read, $write, $field, $bounds; r: $value $(, $($rest)*)?);
    };
    (@codec [$($attr:tt)*] $read:ty, $write:ty, $field:ident, $bounds:tt;
        $direction:ident: $value:ident { $($variant:ident = $encoding:literal),+ $(,)? } $(, $($rest:tt)*)?
    ) => {
        $($attr)* #[repr(usize)] #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub enum $value { $($variant = $encoding),+ }
        $($attr)* const _: () = {
            $(assert!($crate::macros::csr!(@max $bounds).checked_sub($encoding).is_some(), "field encoding exceeds its width");)+
        };
        $($attr)* impl $crate::value::Value for $value {
            const BITS: u32 = usize::BITS;
            #[inline]
            fn from_bits(bits: usize) -> Self {
                match bits { $($encoding => Self::$variant,)+ _ => unreachable!("non-exhaustive field codec") }
            }
            #[inline]
            fn bits(self) -> usize { self as usize }
        }
        $crate::macros::csr!(@exhaustive [$($attr)*] $bounds; $direction; $($encoding),+);
        $crate::macros::csr!(@codec [$($attr)*] $read, $write, $field, $bounds; $direction: $value $(, $($rest)*)?);
    };
    (@exhaustive [$($attr:tt)*] $bounds:tt; rw; $($encoding:literal),+) => {
        $($attr)* const _: () = {
            let encodings = [$($encoding),+];
            assert!(encodings.len() == $crate::macros::csr!(@max $bounds) + 1, "rw enum must cover every read encoding; use separate r and w types");
        };
    };
    (@exhaustive [$($attr:tt)*] $bounds:tt; w; $($encoding:literal),+) => {};
    (@codec [$($attr:tt)*] $read:ty, $write:ty, $field:ident, $bounds:tt; rw: $value:ty) => {
        $crate::macros::csr!(@codec [$($attr)*] $read, $write, $field, $bounds; r: $value, w: $value);
    };
    (@codec [$($attr:tt)*] $read:ty, $write:ty, $field:ident, $bounds:tt;
        $direction:ident: $value:ty, $($rest:tt)+
    ) => {
        $crate::macros::csr!(@codec [$($attr)*] $read, $write, $field, $bounds; $direction: $value);
        $crate::macros::csr!(@codec [$($attr)*] $read, $write, $field, $bounds; $($rest)+);
    };
    (@codec [$($attr:tt)*] $read:ty, $write:ty, $field:ident, $bounds:tt;) => {};
    (@codec [$($attr:tt)*] $read:ty, $write:ty, $field:ident, [$first:literal $(..= $last:literal)?]; r: $value:ty) => {
        $($attr)* impl $crate::value::ReadField<$read> for $field {
            type Value = $value;
            #[inline]
            fn get(register: $read) -> Self::Value {
                let max = const { $crate::macros::csr!(@max [$first $(..= $last)?]) };
                <$value as $crate::value::Value>::from_bits((register.0 >> $first) & max)
            }
        }
    };
    (@codec [$($attr:tt)*] $read:ty, $write:ty, $field:ident, [$first:literal $(..= $last:literal)?]; w: $value:ty) => {
        $($attr)* impl $crate::value::WriteField<$write> for $field {
            type Value = $value;
            #[inline]
            fn set(register: &mut $write, value: Self::Value) {
                let bits = <$value as $crate::value::Value>::bits(value);
                let max = const { $crate::macros::csr!(@max [$first $(..= $last)?]) };
                assert!(bits <= max, "field value exceeds its declared width");
                register.0 = (register.0 & !(max << $first)) | (bits << $first);
            }
        }
    };
}

/// Declares fallible read probes for selected CSRs.
///
/// The invocation generates a local trait and one implementation for each
/// selected register type.
/// With the trait in scope, `Register::detect()` reads the register and returns
/// the invocation's result type.
/// Detection executes one read-only CSR instruction and emits an `.trap_fixup`
/// record with the same layout, illegal-instruction cause, and `t2`/`t3`
/// continuation protocol as Runtime recovery.
/// A successful read returns `Ok(())`; a recovered illegal instruction returns
/// the supplied error expression.
/// Detection never writes the register.
///
/// The result type and error expression are caller-supplied, so this crate does
/// not depend on Runtime's error type.
/// The macro performs no readiness or privilege check.
/// The generated method is unsafe because its caller must establish a
/// compatible trap entry and recovery.
///
/// # Syntax
///
/// ```text
/// detect! {
///     <attributes> <visibility> trait <name> -> <result type> = <error expression>;
///     <attributes> <register type>,
///     ...
/// }
/// ```
///
/// Attributes on a register type, such as `#[cfg]`, also apply to its
/// implementation.
/// Additional register types can implement an existing trait with
/// `detect! { impl Detect -> Result<(), E> = error; Register, ... }`.
/// On targets other than RISC-V, executing the probe panics.
///
/// ```no_run
/// use csr::machine;
///
/// csr::detect! {
///     trait Detect -> Result<(), ()> = ();
///     machine::CounterInhibit,
/// }
///
/// csr::detect! {
///     impl Detect -> Result<(), ()> = ();
///     machine::CounterEnable,
/// }
///
/// let result = unsafe { machine::CounterInhibit::detect() };
/// let other = unsafe { machine::CounterEnable::detect() };
/// # let _ = (result, other);
/// ```
#[macro_export]
macro_rules! detect {
    (
        $(#[$trait_attr:meta])*
        $visibility:vis trait $trait_name:ident -> $result:ty = $error:expr;
        $(
            $(#[$register_attr:meta])*
            $register:ty
        ),+ $(,)?
    ) => {
        $(#[$trait_attr])*
        $visibility trait $trait_name {
            /// Attempts to read this register on the current hart.
            ///
            /// Detection only reads the register.
            ///
            /// # Safety
            ///
            /// Execute on the intended hart in the intended privilege and
            /// execution context. The register read must have no unwanted
            /// side effects.
            /// A compatible Runtime trap entry must be installed, and its
            /// recovery must recognize the `.trap_fixup` record emitted by
            /// this operation.
            /// This operation performs no readiness check.
            unsafe fn detect() -> $result;
        }

        $crate::detect! {
            impl $trait_name -> $result = $error;
            $($(#[$register_attr])* $register),+
        }
    };

    (
        impl $trait_name:ident -> $result:ty = $error:expr;
        $(
            $(#[$register_attr:meta])*
            $register:ty
        ),+ $(,)?
    ) => {
        $(
            $(#[$register_attr])*
            impl $trait_name for $register {
                #[inline]
                unsafe fn detect() -> $result {
                    const { assert!(<$register>::NUMBER < 0x1000, "CSR number must fit in 12 bits"); }
                    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                    {
                        let cause: usize;
                        // SAFETY: The caller establishes current-hart access and
                        // installs recovery for the emitted fault record. `t2`
                        // and `t3` follow Runtime's continuation protocol.
                        unsafe {
                            core::arch::asm!(
                                "li t2, 0",
                                ".option push",
                                ".option norvc",
                                "2:",
                                "csrr {value}, {csr}",
                                "3:",
                                ".option pop",
                                // Keep this record identical to Runtime's
                                // `FaultSite`: two signed word offsets and a
                                // synchronous-cause mask, aligned to four bytes.
                                concat!(
                                    ".pushsection .trap_fixup, \"a\", @progbits\n",
                                    ".balign 4\n",
                                    ".long 2b - .\n",
                                    ".long 3b - .\n",
                                    ".long {causes}\n",
                                    ".popsection\n",
                                ),
                                csr = const <$register>::NUMBER,
                                causes = const 1u32 << 2,
                                value = out(reg) _,
                                out("t2") cause,
                                out("t3") _,
                            );
                        }
                        if cause == 0 { Ok(()) } else { Err($error) }
                    }
                    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
                    {
                        unimplemented!("CSR detection requires RISC-V")
                    }
                }
            }
        )+
    };
}

pub(crate) use csr;
