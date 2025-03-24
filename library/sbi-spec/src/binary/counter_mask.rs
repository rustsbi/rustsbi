use super::{mask_commons::has_bit, sbi_ret::SbiRegister};

/// Counter index mask structure in SBI function calls for the `PMU` extension §11.
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct CounterMask<T = usize> {
    counter_idx_mask: T,
    counter_idx_base: T,
}

impl<T: SbiRegister> CounterMask<T> {
    /// Special value to ignore the `mask`, and consider all `bit`s as set.
    pub const IGNORE_MASK: T = T::FULL_MASK;

    /// Construct a [CounterMask] from mask value and base counter index.
    #[inline]
    pub const fn from_mask_base(counter_idx_mask: T, counter_idx_base: T) -> Self {
        Self {
            counter_idx_mask,
            counter_idx_base,
        }
    }

    /// Gets the special value for ignoring the `mask` parameter.
    #[inline]
    pub const fn ignore_mask(&self) -> T {
        Self::IGNORE_MASK
    }

    /// Returns `mask` and `base` parameters from the [CounterMask].
    #[inline]
    pub const fn into_inner(self) -> (T, T) {
        (self.counter_idx_mask, self.counter_idx_base)
    }
}

// FIXME: implement for T: SbiRegister once we can implement this using const traits.
// Ref: https://rust-lang.github.io/rust-project-goals/2024h2/const-traits.html
impl CounterMask<usize> {
    /// Returns whether the [CounterMask] contains the provided `counter`.
    #[inline]
    pub const fn has_bit(self, counter: usize) -> bool {
        has_bit(
            self.counter_idx_mask,
            self.counter_idx_base,
            Self::IGNORE_MASK,
            counter,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustsbi_counter_index_mask() {
        let mask = CounterMask::from_mask_base(0b1, 400);
        assert!(!mask.has_bit(0));
        assert!(mask.has_bit(400));
        assert!(!mask.has_bit(401));
        let mask = CounterMask::from_mask_base(0b110, 500);
        assert!(!mask.has_bit(0));
        assert!(!mask.has_bit(500));
        assert!(mask.has_bit(501));
        assert!(mask.has_bit(502));
        assert!(!mask.has_bit(500 + (usize::BITS as usize)));
        let max_bit = 1 << (usize::BITS - 1);
        let mask = CounterMask::from_mask_base(max_bit, 600);
        assert!(mask.has_bit(600 + (usize::BITS as usize) - 1));
        assert!(!mask.has_bit(600 + (usize::BITS as usize)));
        let mask = CounterMask::from_mask_base(0b11, usize::MAX - 1);
        assert!(!mask.has_bit(usize::MAX - 2));
        assert!(mask.has_bit(usize::MAX - 1));
        assert!(mask.has_bit(usize::MAX));
        assert!(!mask.has_bit(0));
        let mask = CounterMask::from_mask_base(0, usize::MAX);
        let null_mask = CounterMask::from_mask_base(0, 0);
        (0..=usize::BITS as usize).for_each(|i| {
            assert!(mask.has_bit(i));
            assert!(!null_mask.has_bit(i));
        });
        assert!(mask.has_bit(usize::MAX));
    }

    #[test]
    fn rustsbi_counter_mask_non_usize() {
        assert_eq!(CounterMask::<i32>::IGNORE_MASK, -1);
        assert_eq!(CounterMask::<i64>::IGNORE_MASK, -1);
        assert_eq!(CounterMask::<i128>::IGNORE_MASK, -1);
        assert_eq!(CounterMask::<u32>::IGNORE_MASK, u32::MAX);
        assert_eq!(CounterMask::<u64>::IGNORE_MASK, u64::MAX);
        assert_eq!(CounterMask::<u128>::IGNORE_MASK, u128::MAX);
    }
}
