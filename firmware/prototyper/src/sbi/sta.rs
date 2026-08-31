use core::sync::atomic::{AtomicUsize, Ordering};

use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;

use crate::cfg::NUM_HART_MAX;
use crate::riscv::current_hartid;

struct StaShmem {
    lo: AtomicUsize,
    hi: AtomicUsize,
}

impl StaShmem {
    const DISABLED: Self = Self {
        lo: AtomicUsize::new(usize::MAX),
        hi: AtomicUsize::new(usize::MAX),
    };

    fn store(&self, lo: usize, hi: usize) {
        self.lo.store(lo, Ordering::Release);
        self.hi.store(hi, Ordering::Release);
    }
}

// Keep both address parts since either may be used to represent a physical
// address on RV32. Both parts being all-ones represents a disabled SHMEM.
static STA_SHMEM: [StaShmem; NUM_HART_MAX] = [const { StaShmem::DISABLED }; NUM_HART_MAX];

/// Steal-time Accounting extension using supervisor-provided shared memory.
pub(crate) struct SbiSta;

impl rustsbi::Sta for SbiSta {
    fn set_shmem(&self, shmem: SharedPtr<[u8; 64]>, flags: usize) -> SbiRet {
        if flags != 0 {
            return SbiRet::invalid_param();
        }

        let lo = shmem.phys_addr_lo();
        let hi = shmem.phys_addr_hi();

        // All-ones shared pointer disables steal-time reporting.
        if hi == usize::MAX && lo == usize::MAX {
            STA_SHMEM[current_hartid()].store(lo, hi);
            return SbiRet::success(0);
        }

        if lo & 0x3f != 0 {
            return SbiRet::invalid_param();
        }
        if hi != 0 {
            return SbiRet::invalid_address();
        }

        if !crate::firmware::supervisor_writable(lo, 64) {
            return SbiRet::invalid_address();
        }

        // Clear the structure before returning success.
        // SAFETY: the validated 64-byte range is writable and lies outside
        // firmware memory.
        unsafe {
            core::ptr::write_bytes(lo as *mut u8, 0, 64);
            core::sync::atomic::fence(Ordering::SeqCst);
        }

        STA_SHMEM[current_hartid()].store(lo, hi);
        SbiRet::success(0)
    }
}
