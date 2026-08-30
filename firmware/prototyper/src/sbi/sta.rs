use core::sync::atomic::{AtomicUsize, Ordering};

use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;

use crate::cfg::NUM_HART_MAX;
use crate::riscv::current_hartid;

// A zero address disables reporting for that hart.
static STA_SHMEM: [AtomicUsize; NUM_HART_MAX] = [const { AtomicUsize::new(0) }; NUM_HART_MAX];

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
            STA_SHMEM[current_hartid()].store(0, Ordering::Release);
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

        STA_SHMEM[current_hartid()].store(lo, Ordering::Release);
        SbiRet::success(0)
    }
}
