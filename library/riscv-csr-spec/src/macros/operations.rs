//! Permitted whole-value and bit operations.

macro_rules! __csr_write_spec {
    ($(const $Index:ident: u8;)? $Reg:ty, $Value:ty) => {
        __csr_write_spec!($(const $Index: u8;)? $Reg, $Value, value =>
            $crate::WriteOp::Replace(<$Value as $crate::ScalarCodec>::into_raw_u64(value))
        );
    };
    ($(const $Index:ident: u8;)? $Reg:ty, $Value:ty, $value:ident => $encode:expr) => {
        impl$(<const $Index: u8>)? $crate::sealed::WriteAllowed<$Value> for $Reg {}
        impl$(<const $Index: u8>)? $crate::WriteSpec<$Value> for $Reg {
            type Output = ();

            #[inline(always)]
            fn encode_write($value: $Value) -> $crate::WriteOp {
                $encode
            }

            #[inline(always)]
            fn decode_output(_raw: usize) {}
        }
    };
}

macro_rules! __csr_field_write {
    ($Reg:ident, $word:ident, $Operand:ident, $Permission:ident, $Op:ident, $Out:ty, $decode:expr) => {
        impl<R, W, B, const LO: u32>
            $crate::sealed::WriteAllowed<
                $crate::$Operand<$crate::Field<Self, $word, R, W, B, LO, LO>>,
            > for $Reg
        where
            B: $crate::capability::$Permission,
        {
        }
        impl<R, W, B, const LO: u32>
            $crate::WriteSpec<$crate::$Operand<$crate::Field<Self, $word, R, W, B, LO, LO>>>
            for $Reg
        where
            B: $crate::capability::$Permission,
        {
            type Output = $Out;

            const ATOMIC_NUMBER: u16 = if LO < usize::BITS {
                Self::NUMBER
            } else {
                match <Self as $crate::Csr>::WIDTH {
                    $crate::Width::Split64 { high } => high,
                    _ => panic!("field exceeds its CSR width"),
                }
            };

            #[inline(always)]
            fn encode_write(
                _: $crate::$Operand<$crate::Field<Self, $word, R, W, B, LO, LO>>,
            ) -> $crate::WriteOp {
                $crate::WriteOp::$Op(1usize << (LO % usize::BITS))
            }

            #[inline(always)]
            fn decode_output(raw: usize) -> Self::Output {
                ($decode)(raw)
            }
        }

        __csr_mask_write!($Reg, $word, $Operand, $Permission, $Op, $Out, $decode);
    };
}

macro_rules! __csr_mask_write {
    // A mask spanning both words of a paired register has no single RV32 instruction.
    ($Reg:ident, u64, $($operation:tt)*) => {};
    ($Reg:ident, $word:ident, $Operand:ident, $Permission:ident, $Op:ident, $Out:ty, $decode:expr) => {
        impl<B> $crate::sealed::WriteAllowed<$crate::$Operand<$crate::Mask<Self, B>>> for $Reg where
            B: $crate::capability::$Permission
        {
        }
        impl<B> $crate::WriteSpec<$crate::$Operand<$crate::Mask<Self, B>>> for $Reg
        where
            B: $crate::capability::$Permission,
        {
            type Output = $Out;

            #[inline(always)]
            fn encode_write(value: $crate::$Operand<$crate::Mask<Self, B>>) -> $crate::WriteOp {
                $crate::WriteOp::$Op(value.0.bits())
            }

            #[inline(always)]
            fn decode_output(raw: usize) -> Self::Output {
                ($decode)(raw)
            }
        }
    };
}

macro_rules! __csr_emit_bit_ops {
    ($Reg:ident, $word:ident, none) => {};
    ($Reg:ident, $word:ident, set_only) => {
        __csr_field_write!($Reg, $word, Set, AllowSet, Set, (), |_| {});
    };
    ($Reg:ident, $word:ident, clear_only) => {
        __csr_field_write!($Reg, $word, Clear, AllowClear, Clear, (), |_| {});
    };
    ($Reg:ident, $word:ident, set_clear) => {
        __csr_emit_bit_ops!($Reg, $word, set_only);
        __csr_emit_bit_ops!($Reg, $word, clear_only);
    };
    ($Reg:ident, usize, set_read_and_clear) => {
        __csr_emit_bit_ops!($Reg, usize, set_only);
        __csr_field_write!($Reg, usize, Clear, AllowClear, ReadAndClear, Self, |raw| {
            Self::from_bits(raw)
        });
    };
}
