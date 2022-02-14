/// Hart mask structure reference
#[derive(Debug, Clone)]
pub struct HartMask {
    inner: MaskInner,
}

impl HartMask {
    /// Construct a hart mask from mask value and base hart id.
    #[inline]
    pub fn from_mask_base(hart_mask: usize, hart_mask_base: usize) -> HartMask {
        HartMask {
            inner: MaskInner::BitVector {
                hart_mask,
                hart_mask_base,
            }
        }
    }
    /// Check if the `hart_id` is included in this hart mask structure.
    #[inline]
    pub fn has_bit(&self, hart_id: usize) -> bool {
        match self.inner {
            MaskInner::BitVector { hart_mask, hart_mask_base } => {
                if hart_mask_base == usize::MAX {
                    // If `hart_mask_base` equals `usize::MAX`, that means `hart_mask` is ignored
                    // and all available harts must be considered.
                    return true;
                }
                let idx = if let Some(idx) = hart_id.checked_sub(hart_mask_base) {
                    idx
                } else {
                    // hart_id < hart_mask_base, not in current mask range
                    return false;
                };
                if idx >= usize::BITS as usize {
                    // hart_idx >= hart_mask_base + XLEN, not in current mask range
                    return false;
                }
                hart_mask & (1 << idx) != 0
            },
            MaskInner::Legacy { legacy_bit_vector } => {
                slow_legacy_has_bit(legacy_bit_vector, hart_id)
            },
        }
    }

    /// *This is a legacy function; it should not be used in newer designs. If `vaddr` is invalid
    /// from S level, it would result in machine level load access or load misaligned exception.*
    ///
    /// Construct a hart mask from legacy bit vector and number of harts in current platform.
    #[inline]
    pub(crate) unsafe fn legacy_from_addr(vaddr: usize) -> HartMask {
        HartMask {
            inner: MaskInner::Legacy {
                legacy_bit_vector: vaddr as *const _,
            }
        }
    }
}

#[derive(Debug, Clone)]
enum MaskInner {
    BitVector {
        hart_mask: usize,
        hart_mask_base: usize,
    },
    Legacy {
        legacy_bit_vector: *const usize,
    },
}

// not #[inline] to speed up new version bit vector
fn slow_legacy_has_bit(legacy_bit_vector: *const usize, hart_id: usize) -> bool {
    fn split_index_usize(index: usize) -> (usize, usize) {
        let bits_in_usize = usize::BITS as usize;
        (index / bits_in_usize, index % bits_in_usize)
    }
    let (i, j) = split_index_usize(hart_id);
    let cur_vector = unsafe { get_vaddr_usize(legacy_bit_vector.add(i)) };
    cur_vector & (1 << j) != 0
}

#[inline]
unsafe fn get_vaddr_usize(vaddr_ptr: *const usize) -> usize {
    match () {
        #[cfg(target_arch = "riscv32")]
        () => {
            let mut ans: usize;
            core::arch::asm!("
                li      {tmp}, (1 << 17)
                csrrs   {tmp}, mstatus, {tmp}
                lw      {ans}, 0({vmem})
                csrw    mstatus, {tmp}
            ", ans = lateout(reg) ans, vmem = in(reg) vaddr_ptr, tmp = out(reg) _);
            ans
        }
        #[cfg(target_arch = "riscv64")]
        () => {
            let mut ans: usize;
            core::arch::asm!("
                li      {tmp}, (1 << 17)
                csrrs   {tmp}, mstatus, {tmp}
                ld      {ans}, 0({vmem})
                csrw    mstatus, {tmp}
            ", ans = lateout(reg) ans, vmem = in(reg) vaddr_ptr, tmp = out(reg) _);
            ans
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => {
            drop(vaddr_ptr);
            unimplemented!("not RISC-V instruction set architecture")
        }
    }
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
