use core::{fmt, marker::PhantomData, ops::BitOr};

use crate::capability;

/// Decoding contract for reading a typed field from a CSR snapshot.
#[doc(hidden)]
pub trait ReadFieldValue: Copy {
    /// Decodes a right-aligned raw field integer into the typed read value.
    fn from_field(raw: usize) -> Self;
}

/// Encoding contract for writing a typed field into a prepared CSR value.
#[doc(hidden)]
pub trait WriteFieldValue: Copy {
    /// Encodes this typed write value into its right-aligned raw integer bits.
    fn into_field(self) -> usize;
}

impl ReadFieldValue for bool {
    #[inline(always)]
    fn from_field(raw: usize) -> Self {
        (raw & 1) != 0
    }
}
impl WriteFieldValue for bool {
    #[inline(always)]
    fn into_field(self) -> usize {
        self as usize
    }
}

impl ReadFieldValue for u8 {
    #[inline(always)]
    fn from_field(raw: usize) -> Self {
        raw as u8
    }
}
impl WriteFieldValue for u8 {
    #[inline(always)]
    fn into_field(self) -> usize {
        self as usize
    }
}

impl<T: Copy + TryFrom<usize, Error = usize>> ReadFieldValue for Result<T, usize> {
    #[inline(always)]
    fn from_field(raw: usize) -> Self {
        T::try_from(raw)
    }
}

/// A field spanning bits `LO` through `HI` of register `C`, inclusive.
///
/// `Wd` is the register's raw word type (`usize`, `u32`, or `u64`).
/// `R` is the value returned by `get`. `W` is the value accepted by `set`,
/// or `()` for a field without a setter. `B` selects the allowed atomic
/// set and clear operations from [`capability`].
#[expect(
    clippy::type_complexity,
    reason = "field descriptors carry the register, word, read, write, and bit capability types"
)]
pub struct Field<C, Wd, R, W, B, const LO: u32, const HI: u32>(
    PhantomData<fn() -> (C, Wd, R, W, B)>,
);

impl<C, Wd, R, W, B, const LO: u32, const HI: u32> Field<C, Wd, R, W, B, LO, HI> {
    #[inline(always)]
    pub(crate) const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<C, R, W, B, const LO: u32, const HI: u32> Field<C, usize, R, W, B, LO, HI> {
    /// Returns the architectural XLEN bitmask of this field.
    #[inline(always)]
    pub const fn mask(self) -> usize {
        (usize::MAX >> (usize::BITS - 1 - (HI - LO))) << LO
    }
}

impl<C, R, W, B, const LO: u32, const HI: u32> Field<C, u64, R, W, B, LO, HI> {
    /// Returns the architectural 64-bit mask of this field.
    #[inline(always)]
    pub const fn mask(self) -> u64 {
        (u64::MAX >> (u64::BITS - 1 - (HI - LO))) << LO
    }
}

impl<C, R, W, B, const LO: u32, const HI: u32> Field<C, u32, R, W, B, LO, HI> {
    /// Returns the architectural 32-bit mask of this field.
    #[inline(always)]
    pub const fn mask(self) -> u32 {
        (u32::MAX >> (u32::BITS - 1 - (HI - LO))) << LO
    }
}

impl<C, Wd, R, W, B, const LO: u32, const HI: u32> Clone for Field<C, Wd, R, W, B, LO, HI> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

impl<C, Wd, R, W, B, const LO: u32, const HI: u32> Copy for Field<C, Wd, R, W, B, LO, HI> {}

impl<C, Wd, R, W, B, const LO: u32, const HI: u32> fmt::Debug for Field<C, Wd, R, W, B, LO, HI> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Field({LO}..={HI})")
    }
}

impl<C, Wd, R, W, B, const LO: u32, const HI: u32> Eq for Field<C, Wd, R, W, B, LO, HI> {}

impl<C, Wd, R, W, B, const LO: u32, const HI: u32> PartialEq for Field<C, Wd, R, W, B, LO, HI> {
    #[inline(always)]
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

/// Composed bitmask of one or more single-bit fields belonging to CSR `C`.
#[repr(transparent)]
pub struct Mask<C, B>(usize, PhantomData<fn() -> (C, B)>);

impl<C, B> Mask<C, B> {
    /// Returns the composed raw bitmask for CSR `C`.
    #[inline(always)]
    pub const fn bits(self) -> usize {
        self.0
    }
}

impl<C, B> Clone for Mask<C, B> {
    #[inline(always)]
    fn clone(&self) -> Self {
        *self
    }
}

impl<C, B> Copy for Mask<C, B> {}

impl<C, B> fmt::Debug for Mask<C, B> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Mask").field(&self.0).finish()
    }
}

impl<C, B> Eq for Mask<C, B> {}

impl<C, B> PartialEq for Mask<C, B> {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

macro_rules! field_mask_ops {
    ($word:ty) => {
        impl<C, R1, W1, R2, W2, B, const L1: u32, const L2: u32>
            BitOr<Field<C, $word, R2, W2, B, L2, L2>> for Field<C, $word, R1, W1, B, L1, L1>
        where
            B: capability::AllowBitOp,
        {
            type Output = Mask<C, B>;

            #[inline(always)]
            fn bitor(self, _rhs: Field<C, $word, R2, W2, B, L2, L2>) -> Self::Output {
                Mask((1usize << L1) | (1usize << L2), PhantomData)
            }
        }

        impl<C, R, W, B, const LO: u32> BitOr<Mask<C, B>> for Field<C, $word, R, W, B, LO, LO>
        where
            B: capability::AllowBitOp,
        {
            type Output = Mask<C, B>;

            #[inline(always)]
            fn bitor(self, rhs: Mask<C, B>) -> Self::Output {
                Mask((1usize << LO) | rhs.0, PhantomData)
            }
        }

        impl<C, R, W, B, const LO: u32> BitOr<Field<C, $word, R, W, B, LO, LO>> for Mask<C, B>
        where
            B: capability::AllowBitOp,
        {
            type Output = Mask<C, B>;

            #[inline(always)]
            fn bitor(self, _rhs: Field<C, $word, R, W, B, LO, LO>) -> Self::Output {
                Mask(self.0 | (1usize << LO), PhantomData)
            }
        }
    };
}

field_mask_ops!(usize);
field_mask_ops!(u32);

impl<C, B> BitOr<Mask<C, B>> for Mask<C, B>
where
    B: capability::AllowBitOp,
{
    type Output = Mask<C, B>;

    #[inline(always)]
    fn bitor(self, rhs: Mask<C, B>) -> Self::Output {
        Mask(self.0 | rhs.0, PhantomData)
    }
}
