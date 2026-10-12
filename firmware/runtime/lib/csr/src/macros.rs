//! CSR declaration parser and register generator.
//!
//! `Name { number, RO/RW, value: Type }` separates the register from its value.
//! Declarations must have no read side effects that can violate memory safety.
//! `RW` generates safe writes; registers with additional write invariants
//! implement their writes separately.

macro_rules! csr {
    () => {};
    (
        $(#[$attr:meta])* $name:ident { $number:literal, $access:ident, value: $value:ty $(,)? }
        $($rest:tt)*
    ) => {
        $(#[$attr])*
        pub enum $name {}

        $(#[$attr])*
        const _: () = {
            let number: ::core::primitive::u16 = $number;
            ::core::assert!(number < 0x1000, "CSR number must fit in 12 bits");
            let width = <$value as $crate::value::Value>::BITS;
            ::core::assert!(
                width > 0 && width <= ::core::primitive::usize::BITS,
                "CSR value must fit in one XLEN-wide access",
            );
        };

        $crate::macros::csr!(@read [$(#[$attr])*] $name, $number, $value);
        $crate::macros::csr!(@write [$(#[$attr])*] $name, $number, $access, $value);
        $crate::macros::csr!($($rest)*);
    };

    (@read [$($attr:tt)*] $name:ident, $number:literal, $value:ty) => {
        $($attr)*
        impl $name {
            /// Reads the hardware register on the current hart.
            /// May trap if the register is absent or the current privilege is insufficient.
            #[inline]
            pub fn read() -> $value {
                #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                {
                    let bits: ::core::primitive::usize;
                    // SAFETY: declared CSRs have no reads that violate memory safety.
                    unsafe {
                        ::core::arch::asm!(
                            "csrr {bits}, {csr}",
                            bits = out(reg) bits,
                            csr = const $number,
                            options(nostack),
                        );
                    }
                    <$value as $crate::value::Value>::from_bits(bits)
                }
                #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
                ::core::panic!("CSR access requires RISC-V")
            }
        }
    };

    (@write [$($attr:tt)*] $name:ident, $number:literal, RO, $value:ty) => {};
    (@write [$($attr:tt)*] $name:ident, $number:literal, RW, $value:ty) => {
        $($attr)*
        const _: () = ::core::assert!(
            $number & 0xc00 != 0xc00,
            "read-only CSR encoding cannot declare write access",
        );

        $($attr)*
        impl $name {
            /// Writes the complete hardware register.
            /// May trap if the register is absent or the current privilege is insufficient.
            #[inline]
            pub fn write(value: $value) {
                #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                // SAFETY: RW declarations permit arbitrary values without
                // additional memory-safety requirements.
                unsafe {
                    ::core::arch::asm!(
                        "csrw {csr}, {bits}",
                        csr = const $number,
                        bits = in(reg) <$value as $crate::value::Value>::bits(value),
                        options(nostack),
                    );
                }
                #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
                {
                    let _ = value;
                    ::core::panic!("CSR access requires RISC-V")
                }
            }
        }
    };
}

pub(crate) use csr;
