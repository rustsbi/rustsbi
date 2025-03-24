use super::{
    mask_commons::{MaskError, has_bit, valid_bit},
    sbi_ret::SbiRegister,
};

/// Hart mask structure in SBI function calls.
#[repr(C)]
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct HartMask<T = usize> {
    hart_mask: T,
    hart_mask_base: T,
}

impl<T: SbiRegister> HartMask<T> {
    /// Special value to ignore the `mask`, and consider all `bit`s as set.
    pub const IGNORE_MASK: T = T::FULL_MASK;

    /// Construct a [HartMask] from mask value and base hart id.
    #[inline]
    pub const fn from_mask_base(hart_mask: T, hart_mask_base: T) -> Self {
        Self {
            hart_mask,
            hart_mask_base,
        }
    }

    /// Construct a [HartMask] that selects all available harts on the current environment.
    ///
    /// According to the RISC-V SBI Specification, `hart_mask_base` can be set to `-1` (i.e. `usize::MAX`)
    /// to indicate that `hart_mask` shall be ignored and all available harts must be considered.
    /// In case of this function in the `sbi-spec` crate, we fill in `usize::MAX` in `hart_mask_base`
    /// parameter to match the RISC-V SBI standard, while choosing 0 as the ignored `hart_mask` value.
    #[inline]
    pub const fn all() -> Self {
        Self {
            hart_mask: T::ZERO,
            hart_mask_base: T::FULL_MASK,
        }
    }

    /// Gets the special value for ignoring the `mask` parameter.
    #[inline]
    pub const fn ignore_mask(&self) -> T {
        Self::IGNORE_MASK
    }

    /// Returns `mask` and `base` parameters from the [HartMask].
    #[inline]
    pub const fn into_inner(self) -> (T, T) {
        (self.hart_mask, self.hart_mask_base)
    }
}

// FIXME: implement for T: SbiRegister once we can implement this using const traits.
// Ref: https://rust-lang.github.io/rust-project-goals/2024h2/const-traits.html
impl HartMask<usize> {
    /// Returns whether the [HartMask] contains the provided `hart_id`.
    #[inline]
    pub const fn has_bit(self, hart_id: usize) -> bool {
        has_bit(
            self.hart_mask,
            self.hart_mask_base,
            Self::IGNORE_MASK,
            hart_id,
        )
    }

    /// Insert a hart id into this [HartMask].
    ///
    /// Returns error when `hart_id` is invalid.
    #[inline]
    pub const fn insert(&mut self, hart_id: usize) -> Result<(), MaskError> {
        if self.hart_mask_base == Self::IGNORE_MASK {
            Ok(())
        } else if valid_bit(self.hart_mask_base, hart_id) {
            self.hart_mask |= 1usize << (hart_id - self.hart_mask_base);
            Ok(())
        } else {
            Err(MaskError::InvalidBit)
        }
    }

    /// Remove a hart id from this [HartMask].
    ///
    /// Returns error when `hart_id` is invalid, or it has been ignored.
    #[inline]
    pub const fn remove(&mut self, hart_id: usize) -> Result<(), MaskError> {
        if self.hart_mask_base == Self::IGNORE_MASK {
            Err(MaskError::Ignored)
        } else if valid_bit(self.hart_mask_base, hart_id) {
            self.hart_mask &= !(1usize << (hart_id - self.hart_mask_base));
            Ok(())
        } else {
            Err(MaskError::InvalidBit)
        }
    }

    /// Returns [HartIds] of self.
    #[inline]
    pub const fn iter(&self) -> HartIds {
        HartIds {
            inner: match self.hart_mask_base {
                Self::IGNORE_MASK => UnvisitedMask::Range(0, usize::MAX),
                _ => UnvisitedMask::MaskBase(self.hart_mask, self.hart_mask_base),
            },
        }
    }
}

impl IntoIterator for HartMask {
    type Item = usize;

    type IntoIter = HartIds;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterator structure for `HartMask`.
///
/// It will iterate hart id from low to high.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct HartIds {
    inner: UnvisitedMask,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
enum UnvisitedMask {
    MaskBase(usize, usize),
    Range(usize, usize),
}

impl Iterator for HartIds {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            UnvisitedMask::MaskBase(0, _base) => None,
            UnvisitedMask::MaskBase(unvisited_mask, base) => {
                let low_bit = unvisited_mask.trailing_zeros();
                let hart_id = usize::try_from(low_bit).unwrap() + *base;
                *unvisited_mask &= !(1usize << low_bit);
                Some(hart_id)
            }
            UnvisitedMask::Range(start, end) => {
                assert!(start <= end);
                if *start < *end {
                    let ans = *start;
                    *start += 1;
                    Some(ans)
                } else {
                    None
                }
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self.inner {
            UnvisitedMask::MaskBase(unvisited_mask, _base) => {
                let exact_popcnt = usize::try_from(unvisited_mask.count_ones()).unwrap();
                (exact_popcnt, Some(exact_popcnt))
            }
            UnvisitedMask::Range(start, end) => {
                assert!(start <= end);
                let exact_num_harts = end - start;
                (exact_num_harts, Some(exact_num_harts))
            }
        }
    }

    #[inline]
    fn count(self) -> usize {
        self.size_hint().0
    }

    #[inline]
    fn last(mut self) -> Option<Self::Item> {
        self.next_back()
    }

    #[inline]
    fn min(mut self) -> Option<Self::Item> {
        self.next()
    }

    #[inline]
    fn max(mut self) -> Option<Self::Item> {
        self.next_back()
    }

    #[inline]
    fn is_sorted(self) -> bool {
        true
    }

    // TODO: implement fn advance_by once it's stabilized: https://github.com/rust-lang/rust/issues/77404
    // #[inline]
    // fn advance_by(&mut self, n: usize) -> Result<(), core::num::NonZero<usize>> { ... }
}

impl DoubleEndedIterator for HartIds {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            UnvisitedMask::MaskBase(0, _base) => None,
            UnvisitedMask::MaskBase(unvisited_mask, base) => {
                let high_bit = unvisited_mask.leading_zeros();
                let hart_id = usize::try_from(usize::BITS - high_bit - 1).unwrap() + *base;
                *unvisited_mask &= !(1usize << (usize::BITS - high_bit - 1));
                Some(hart_id)
            }
            UnvisitedMask::Range(start, end) => {
                assert!(start <= end);
                if *start < *end {
                    let ans = *end;
                    *end -= 1;
                    Some(ans)
                } else {
                    None
                }
            }
        }
    }

    // TODO: implement advance_back_by once stabilized.
    // #[inline]
    // fn advance_back_by(&mut self, n: usize) -> Result<(), core::num::NonZero<usize>> { ... }
}

impl ExactSizeIterator for HartIds {}

impl core::iter::FusedIterator for HartIds {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rustsbi_hart_mask() {
        let mask = HartMask::from_mask_base(0b1, 400);
        assert!(!mask.has_bit(0));
        assert!(mask.has_bit(400));
        assert!(!mask.has_bit(401));
        let mask = HartMask::from_mask_base(0b110, 500);
        assert!(!mask.has_bit(0));
        assert!(!mask.has_bit(500));
        assert!(mask.has_bit(501));
        assert!(mask.has_bit(502));
        assert!(!mask.has_bit(500 + (usize::BITS as usize)));
        let max_bit = 1 << (usize::BITS - 1);
        let mask = HartMask::from_mask_base(max_bit, 600);
        assert!(mask.has_bit(600 + (usize::BITS as usize) - 1));
        assert!(!mask.has_bit(600 + (usize::BITS as usize)));
        let mask = HartMask::from_mask_base(0b11, usize::MAX - 1);
        assert!(!mask.has_bit(usize::MAX - 2));
        assert!(mask.has_bit(usize::MAX - 1));
        assert!(mask.has_bit(usize::MAX));
        assert!(!mask.has_bit(0));
        // hart_mask_base == usize::MAX is special, it means hart_mask should be ignored
        // and this hart mask contains all harts available
        let mask = HartMask::from_mask_base(0, usize::MAX);
        for i in 0..5 {
            assert!(mask.has_bit(i));
        }
        assert!(mask.has_bit(usize::MAX));

        let mut mask = HartMask::from_mask_base(0, 1);
        assert!(!mask.has_bit(1));
        assert!(mask.insert(1).is_ok());
        assert!(mask.has_bit(1));
        assert!(mask.remove(1).is_ok());
        assert!(!mask.has_bit(1));
    }

    #[test]
    fn rustsbi_hart_ids_iterator() {
        let mask = HartMask::from_mask_base(0b101011, 1);
        // Test the `next` method of `HartIds` structure.
        let mut hart_ids = mask.iter();
        assert_eq!(hart_ids.next(), Some(1));
        assert_eq!(hart_ids.next(), Some(2));
        assert_eq!(hart_ids.next(), Some(4));
        assert_eq!(hart_ids.next(), Some(6));
        assert_eq!(hart_ids.next(), None);
        // `HartIds` structures are fused, meaning they return `None` forever once iteration finished.
        assert_eq!(hart_ids.next(), None);

        // Test `for` loop on mask (`HartMask`) as `IntoIterator`.
        let mut ans = [0; 4];
        let mut idx = 0;
        for hart_id in mask {
            ans[idx] = hart_id;
            idx += 1;
        }
        assert_eq!(ans, [1, 2, 4, 6]);

        // Test `Iterator` methods on `HartIds`.
        let mut hart_ids = mask.iter();
        assert_eq!(hart_ids.size_hint(), (4, Some(4)));
        let _ = hart_ids.next();
        assert_eq!(hart_ids.size_hint(), (3, Some(3)));
        let _ = hart_ids.next();
        let _ = hart_ids.next();
        assert_eq!(hart_ids.size_hint(), (1, Some(1)));
        let _ = hart_ids.next();
        assert_eq!(hart_ids.size_hint(), (0, Some(0)));
        let _ = hart_ids.next();
        assert_eq!(hart_ids.size_hint(), (0, Some(0)));

        let mut hart_ids = mask.iter();
        assert_eq!(hart_ids.count(), 4);
        let _ = hart_ids.next();
        assert_eq!(hart_ids.count(), 3);
        let _ = hart_ids.next();
        let _ = hart_ids.next();
        let _ = hart_ids.next();
        assert_eq!(hart_ids.count(), 0);
        let _ = hart_ids.next();
        assert_eq!(hart_ids.count(), 0);

        let hart_ids = mask.iter();
        assert_eq!(hart_ids.last(), Some(6));

        let mut hart_ids = mask.iter();
        assert_eq!(hart_ids.nth(2), Some(4));
        let mut hart_ids = mask.iter();
        assert_eq!(hart_ids.nth(0), Some(1));

        let mut iter = mask.iter().step_by(2);
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), None);

        let mask_2 = HartMask::from_mask_base(0b1001101, 64);
        let mut iter = mask.iter().chain(mask_2);
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.next(), Some(64));
        assert_eq!(iter.next(), Some(66));
        assert_eq!(iter.next(), Some(67));
        assert_eq!(iter.next(), Some(70));
        assert_eq!(iter.next(), None);

        let mut iter = mask.iter().zip(mask_2);
        assert_eq!(iter.next(), Some((1, 64)));
        assert_eq!(iter.next(), Some((2, 66)));
        assert_eq!(iter.next(), Some((4, 67)));
        assert_eq!(iter.next(), Some((6, 70)));
        assert_eq!(iter.next(), None);

        fn to_plic_context_id(hart_id_machine: usize) -> usize {
            hart_id_machine * 2
        }
        let mut iter = mask.iter().map(to_plic_context_id);
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(8));
        assert_eq!(iter.next(), Some(12));
        assert_eq!(iter.next(), None);

        let mut channel_received = [0; 4];
        let mut idx = 0;
        let mut channel_send = |hart_id| {
            channel_received[idx] = hart_id;
            idx += 1;
        };
        mask.iter().for_each(|value| channel_send(value));
        assert_eq!(channel_received, [1, 2, 4, 6]);

        let is_in_cluster_1 = |hart_id: &usize| *hart_id >= 4 && *hart_id < 7;
        let mut iter = mask.iter().filter(is_in_cluster_1);
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.next(), None);

        let if_in_cluster_1_get_plic_context_id = |hart_id: usize| {
            if hart_id >= 4 && hart_id < 7 {
                Some(hart_id * 2)
            } else {
                None
            }
        };
        let mut iter = mask.iter().filter_map(if_in_cluster_1_get_plic_context_id);
        assert_eq!(iter.next(), Some(8));
        assert_eq!(iter.next(), Some(12));
        assert_eq!(iter.next(), None);

        let mut iter = mask.iter().enumerate();
        assert_eq!(iter.next(), Some((0, 1)));
        assert_eq!(iter.next(), Some((1, 2)));
        assert_eq!(iter.next(), Some((2, 4)));
        assert_eq!(iter.next(), Some((3, 6)));
        assert_eq!(iter.next(), None);
        let mut ans = [(0, 0); 4];
        let mut idx = 0;
        for (i, hart_id) in mask.iter().enumerate() {
            ans[idx] = (i, hart_id);
            idx += 1;
        }
        assert_eq!(ans, [(0, 1), (1, 2), (2, 4), (3, 6)]);

        let mut iter = mask.iter().peekable();
        assert_eq!(iter.peek(), Some(&1));
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.peek(), Some(&2));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.peek(), Some(&4));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.peek(), Some(&6));
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.peek(), None);
        assert_eq!(iter.next(), None);

        // TODO: other iterator tests.

        assert!(mask.iter().is_sorted());
        assert!(mask.iter().is_sorted_by(|a, b| a <= b));

        // Reverse iterator as `DoubleEndedIterator`.
        let mut iter = mask.iter().rev();
        assert_eq!(iter.next(), Some(6));
        assert_eq!(iter.next(), Some(4));
        assert_eq!(iter.next(), Some(2));
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), None);

        // Special iterator values.
        let nothing = HartMask::from_mask_base(0, 1000);
        assert!(nothing.iter().eq([]));

        let all_mask_bits_set = HartMask::from_mask_base(usize::MAX, 1000);
        let range = 1000..(1000 + usize::BITS as usize);
        assert!(all_mask_bits_set.iter().eq(range));

        let all_harts = HartMask::all();
        let mut iter = all_harts.iter();
        assert_eq!(iter.size_hint(), (usize::MAX, Some(usize::MAX)));
        // Don't use `Iterator::eq` here; it would literally run `Iterator::try_for_each` from 0 to usize::MAX
        // which will cost us forever to run the test.
        assert_eq!(iter.next(), Some(0));
        assert_eq!(iter.size_hint(), (usize::MAX - 1, Some(usize::MAX - 1)));
        assert_eq!(iter.next(), Some(1));
        assert_eq!(iter.next(), Some(2));
        // skip 500 elements
        let _ = iter.nth(500 - 1);
        assert_eq!(iter.next(), Some(503));
        assert_eq!(iter.size_hint(), (usize::MAX - 504, Some(usize::MAX - 504)));
        assert_eq!(iter.next_back(), Some(usize::MAX));
        assert_eq!(iter.next_back(), Some(usize::MAX - 1));
        assert_eq!(iter.size_hint(), (usize::MAX - 506, Some(usize::MAX - 506)));

        // A common usage of `HartMask::all`, we assume that this platform filters out hart 0..=3.
        let environment_available_hart_ids = 4..128;
        // `hart_mask_iter` contains 64..=usize::MAX.
        let hart_mask_iter = all_harts.iter().skip(64);
        let filtered_iter = environment_available_hart_ids.filter(|&x| {
            hart_mask_iter
                .clone()
                .find(|&y| y >= x)
                .map_or(false, |y| y == x)
        });
        assert!(filtered_iter.eq(64..128));

        // The following operations should have O(1) complexity.
        let all_harts = HartMask::all();
        assert_eq!(all_harts.iter().count(), usize::MAX);
        assert_eq!(all_harts.iter().last(), Some(usize::MAX));
        assert_eq!(all_harts.iter().min(), Some(0));
        assert_eq!(all_harts.iter().max(), Some(usize::MAX));
        assert!(all_harts.iter().is_sorted());

        let partial_all_harts = {
            let mut ans = HartMask::all().iter();
            let _ = ans.nth(65536 - 1);
            let _ = ans.nth_back(4096 - 1);
            ans
        };
        assert_eq!(partial_all_harts.clone().count(), usize::MAX - 65536 - 4096);
        assert_eq!(partial_all_harts.clone().last(), Some(usize::MAX - 4096));
        assert_eq!(partial_all_harts.clone().min(), Some(65536));
        assert_eq!(partial_all_harts.clone().max(), Some(usize::MAX - 4096));
        assert!(partial_all_harts.is_sorted());

        let nothing = HartMask::from_mask_base(0, 1000);
        assert_eq!(nothing.iter().count(), 0);
        assert_eq!(nothing.iter().last(), None);
        assert_eq!(nothing.iter().min(), None);
        assert_eq!(nothing.iter().max(), None);
        assert!(nothing.iter().is_sorted());

        let mask = HartMask::from_mask_base(0b101011, 1);
        assert_eq!(mask.iter().count(), 4);
        assert_eq!(mask.iter().last(), Some(6));
        assert_eq!(mask.iter().min(), Some(1));
        assert_eq!(mask.iter().max(), Some(6));
        assert!(mask.iter().is_sorted());

        let all_mask_bits_set = HartMask::from_mask_base(usize::MAX, 1000);
        let last = 1000 + usize::BITS as usize - 1;
        assert_eq!(all_mask_bits_set.iter().count(), usize::BITS as usize);
        assert_eq!(all_mask_bits_set.iter().last(), Some(last));
        assert_eq!(all_mask_bits_set.iter().min(), Some(1000));
        assert_eq!(all_mask_bits_set.iter().max(), Some(last));
        assert!(all_mask_bits_set.iter().is_sorted());
    }

    #[test]
    fn rustsbi_hart_mask_non_usize() {
        assert_eq!(HartMask::<i32>::IGNORE_MASK, -1);
        assert_eq!(HartMask::<i64>::IGNORE_MASK, -1);
        assert_eq!(HartMask::<i128>::IGNORE_MASK, -1);
        assert_eq!(HartMask::<u32>::IGNORE_MASK, u32::MAX);
        assert_eq!(HartMask::<u64>::IGNORE_MASK, u64::MAX);
        assert_eq!(HartMask::<u128>::IGNORE_MASK, u128::MAX);

        assert_eq!(HartMask::<i32>::all(), HartMask::from_mask_base(0, -1));
    }
}
