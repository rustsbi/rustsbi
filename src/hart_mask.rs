/// Hart mask structure reference
#[derive(Debug, Clone)]
pub struct HartMask {
    inner: BitVector,
}

impl HartMask {
    /// Construct a hart mask from mask value and base hart id.
    #[inline]
    pub const fn from_mask_base(hart_mask: usize, hart_mask_base: usize) -> HartMask {
        HartMask {
            inner: BitVector {
                hart_mask,
                hart_mask_base,
            },
        }
    }

    /// Check if the `hart_id` is included in this hart mask structure.
    #[inline]
    pub const fn has_bit(&self, hart_id: usize) -> bool {
        let BitVector {
            hart_mask,
            hart_mask_base,
        } = self.inner;
        if hart_mask_base == usize::MAX {
            // If `hart_mask_base` equals `usize::MAX`, that means `hart_mask` is ignored
            // and all available harts must be considered.
            return true;
        }
        let Some(idx) = hart_id.checked_sub(hart_mask_base) else {
            // hart_id < hart_mask_base, not in current mask range
            return false;
        };
        if idx >= usize::BITS as usize {
            // hart_idx >= hart_mask_base + XLEN, not in current mask range
            return false;
        }
        hart_mask & (1 << idx) != 0
    }
}

#[derive(Debug, Clone)]
struct BitVector {
    hart_mask: usize,
    hart_mask_base: usize,
}

#[cfg(test)]
mod tests {
    use super::HartMask;

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
    }
}
