use rustsbi::SbiRet;
use sbi_spec::binary::SharedPtr;

/// Steal-time Accounting extension implementation.
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
            return SbiRet::success(0);
        }

        // STA requires a 64-byte aligned native physical address.
        if lo & 0x3f != 0 || hi != 0 {
            return SbiRet::invalid_param();
        }

        if !crate::firmware::supervisor_writable(lo, 64) {
            return SbiRet::invalid_address();
        }

        // The structure is reset before the supervisor observes success.
        // Safety: `lo` was validated above (writable, aligned, outside
        // firmware memory); the write is volatile so it cannot be elided.
        unsafe {
            core::ptr::write_bytes(lo as *mut u8, 0, 64);
            core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        }

        SbiRet::success(0)
    }
}
