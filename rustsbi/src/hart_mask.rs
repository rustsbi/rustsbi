use core::mem::size_of;

/// Hart mask structure reference
#[derive(Debug, Clone)]
pub struct HartMask {
    bit_vector: *const usize,
    base: usize,
    max_hart_id: usize,
}

impl HartMask {
    /// Construct a reference to hart mask from bit vector and starting hartid.
    ///
    /// # Parameters
    ///
    /// - The `vaddr` is a scalar bit-vector containing hartids. 
    ///   Should return address from supervisor level.
    /// - The `base` is the starting hartid from which bit-vector must be computed.
    ///   If `base` equals `usize::max_value()`, that means `vaddr` is ignored and all available harts must be considered.
    /// - The `max_hart_id` should be returned by SBI implementation for maximum hart id this hart mask supports.
    ///
    /// # Unsafety
    ///
    /// Caller must ensure all usize values in the bit vector is accessible.
    pub unsafe fn from_addr(vaddr: usize, base: usize, max_hart_id: usize) -> HartMask {
        HartMask {
            bit_vector: vaddr as *const usize,
            base,
            max_hart_id,
        }
    }

    /// Check if the `hart_id` is included in this hart mask structure.
    pub fn has_bit(&self, hart_id: usize) -> bool {
        assert!(hart_id <= self.max_hart_id);
        if self.base == usize::max_value() {
            // If `base` equals `usize::max_value()`, 
            // that means `vaddr` is ignored and all available harts must be considered.
            return true;
        }
        if hart_id < self.base {
            // `base` if the starting hartid
            return false;
        }
        let (i, j) = split_index_usize(hart_id - self.base);
        let cur_vector = unsafe { get_vaddr_usize(self.bit_vector.add(i)) };
        cur_vector & (1 << j) != 0
    }
}

#[inline]
fn split_index_usize(index: usize) -> (usize, usize) {
    let bits_in_usize = size_of::<usize>() * 8;
    (index / bits_in_usize, index % bits_in_usize)
}

#[inline]
unsafe fn get_vaddr_usize(vaddr_ptr: *const usize) -> usize {
    match () {
        #[cfg(target_arch = "riscv32")]
        () => {
            let mut ans: usize;
            asm!("
                li      {tmp}, (1 << 17)
                csrrs   {tmp}, mstatus, {tmp}
                lw      {ans}, 0({vmem})
                csrw    mstatus, {tmp}
            ", ans = lateout(reg) ans, vmem = in(reg) vaddr_ptr, tmp = out(reg) _);
            ans
        },
        #[cfg(target_arch = "riscv64")]
        () => {
            let mut ans: usize;
            asm!("
                li      {tmp}, (1 << 17)
                csrrs   {tmp}, mstatus, {tmp}
                ld      {ans}, 0({vmem})
                csrw    mstatus, {tmp}
            ", ans = lateout(reg) ans, vmem = in(reg) vaddr_ptr, tmp = out(reg) _);
            ans
        },
        #[cfg(target_arch = "riscv128")]
        () => {
            let mut ans: usize;
            asm!("
                li      {tmp}, (1 << 17)
                csrrs   {tmp}, mstatus, {tmp}
                lq      {ans}, 0({vmem})
                csrw    mstatus, {tmp}
            ", ans = lateout(reg) ans, vmem = in(reg) vaddr_ptr, tmp = out(reg) _);
            ans
        },
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        () => {
            drop(vaddr_ptr);
            unimplemented!("not RISC-V instruction set architecture")
        }
    }
}
