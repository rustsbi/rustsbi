//! Standard RISC-V architecture and extension specifications.

// Standard `r` declarations also authorize the executor's safe read.
macro_rules! impl_read_spec {
    ($($definition:tt)*) => {
        $crate::macros::impl_read_spec!(standard; $($definition)*);
    };
}

macro_rules! interrupt_bits {
    ($($register:ident $(<$mode:ident>)?),+) => {$(
        impl$(<$mode>)? $register$(<$mode>)? {
            /// Returns whether the bit for `number` is set in this interrupt register.
            ///
            /// Numbers beyond the width of the register return `false`.
            #[inline(always)]
            pub const fn bit(self, number: usize) -> bool {
                number < usize::BITS as usize && self.0 & (1usize << number) != 0
            }
        }
    )+};
}

pub mod aia;
pub mod isa;
